use clap::Parser;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File};
use std::io::{self, Write as IoWrite};
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::time::sleep;
use walkdir::WalkDir;
use glob::glob;
use rodio::{OutputStream, Sink, Source, source::SineWave, Decoder};
use std::time::Duration as StdDuration;
use std::io::Cursor;
use log;  // New import for logging

const GROK_RESPONSE_MARKER: &str = "GROK RESPONSE";
const USER_PROMPT_MARKER: &str = "USER PROMPT";
const THINKING_MESSAGE: &str = "Grok is thinking...";
const MAX_LEVEL: u32 = 5; // Corresponds to L5 = 16384, as per original default

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short = 'f', long, default_value = "./gchat.md")]
    chat_file: String,

    #[arg(short = 't', long, default_value = "L5")]
    max_tokens: String,

    #[arg(short = 'T', long, default_value = "600")]
    api_timeout: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize, Debug)]  // Added Debug derive for logging
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::init();  // Initialize logging (configure via RUST_LOG env var)

    let args = Args::parse();

    // Parse the max_tokens level
    let max_tokens = match parse_level(&args.max_tokens) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing --max-tokens: {}", e);
            std::process::exit(1);
        }
    };

    let chat_path = PathBuf::from(&args.chat_file);

    // Create chat file if it doesn't exist
    if !chat_path.exists() {
        let mut file = File::create(&chat_path)?;
        writeln!(file, "{}:\n", USER_PROMPT_MARKER)?;
        println!(
            "Created chat file at {}. Start your conversation by adding:\n{}:\nYour prompt here\n",
            chat_path.display(), USER_PROMPT_MARKER
        );
    }

    // Print settings on startup
    println!("Running with settings:");
    println!("  Chat file: {}", args.chat_file);
    println!("  Max tokens: {} ({})", args.max_tokens, max_tokens);
    println!("  API timeout: {} seconds", args.api_timeout);

    println!("App started. Polling {} for changes every 1 second.", args.chat_file);

    // Initial process on startup
    if let Err(e) = process_chat_file(
        &chat_path,
        max_tokens,
        args.api_timeout,
    )
    .await
    {
        println!("Processing error: {}", e);
    }

    // Get initial modification time (or now if unavailable)
    let mut last_mtime = fs::metadata(&chat_path)
        .and_then(|meta| meta.modified())
        .unwrap_or(SystemTime::now());

    // Polling loop
    loop {
        // Sleep for 1 second between checks
        sleep(Duration::from_secs(1)).await;

        // Get current modification time
        let current_mtime = match fs::metadata(&chat_path) {
            Ok(meta) => match meta.modified() {
                Ok(mtime) => mtime,
                Err(_) => continue, // Skip if can't get mtime
            },
            Err(_) => continue, // Skip if file doesn't exist temporarily
        };

        if current_mtime > last_mtime {
            // File changed: process it
            if let Err(e) = process_chat_file(
                &chat_path,
                max_tokens,
                args.api_timeout,
            )
            .await
            {
                println!("Processing error: {}", e);
            }
            // Update last mtime after processing
            last_mtime = current_mtime;
        }
    }
}

fn parse_level(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if let Some(lstr) = s.strip_prefix('L') {
        match lstr.parse::<u32>() {
            Ok(level) if level <= MAX_LEVEL => Ok(512u32 << level),
            Ok(level) => Err(format!(
                "Level too high: L{}, max L{} ({} tokens)",
                level,
                MAX_LEVEL,
                512u32 << MAX_LEVEL
            )),
            Err(_) => Err("Invalid level: expected L followed by a number (e.g., L5)".to_string()),
        }
    } else {
        Err("Invalid format: expected L<level> (e.g., L5)".to_string())
    }
}

async fn process_chat_file(
    chat_path: &PathBuf,
    max_tokens: u32,
    api_timeout: u64,
) -> io::Result<()> {
    // Short debounce to ensure save is complete (helps with atomic saves)
    sleep(Duration::from_millis(500)).await;

    let content = fs::read_to_string(chat_path)?;
    let mut messages = parse_chat_messages(&content);

    if messages.is_empty() || messages.last().unwrap().role != "user" || messages.last().unwrap().content.trim().is_empty() {
        println!("No complete user prompt to process in chat file.");
        return Ok(()); // No send needed
    }

    // Handle @t placeholders: remove from all user messages, and set local_max_tokens from the last @t in the last user message
    let mut local_max_tokens = max_tokens;
    let re = Regex::new(r"@t\s*:\s*L(\d+)").unwrap();
    for i in 0..messages.len() {
        if messages[i].role == "user" {
            let content = &messages[i].content;
            let mut new_content = content.to_string();
            let mut last_level: Option<u32> = None;
            let mut ranges = vec![];
            for cap in re.captures_iter(content) {
                let whole = cap.get(0).unwrap();
                ranges.push(whole.range());
                if let Some(num_str) = cap.get(1) {
                    if let Ok(lvl) = num_str.as_str().parse::<u32>() {
                        last_level = Some(lvl);
                    }
                }
            }
            // Remove in reverse order to avoid index issues
            for range in ranges.into_iter().rev() {
                new_content.replace_range(range, "");
            }
            messages[i].content = new_content;
            // If this is the last message, apply the last_level if present
            if i == messages.len() - 1 {
                if let Some(lvl) = last_level {
                    let mut effective_lvl = lvl;
                    if effective_lvl > MAX_LEVEL {
                        println!(
                            "Warning: Specified level L{} too high, capping at L{} ({} tokens)",
                            lvl,
                            MAX_LEVEL,
                            512u32 << MAX_LEVEL
                        );
                        effective_lvl = MAX_LEVEL;
                    }
                    local_max_tokens = 512u32 << effective_lvl;
                    println!("Setting `max_tokens` API parameter to {}", local_max_tokens);
                }
            }
        }
    }

    // Expand other placeholders ONLY in user messages (prompts to the API)
    for msg in messages.iter_mut() {
        if msg.role == "user" {
            msg.content = expand_placeholders(&msg.content)?;
        }
    }

    // Log the expanded messages (DEBUG level)
    log::debug!("Expanded messages for API request: {:?}", messages);

    // Get API key, build client, create req
    let api_key = env::var("XAI_API_KEY").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "XAI_API_KEY not set"))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(api_timeout))
        .build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let req = ChatRequest {
        model: "grok-4-0709".to_string(),
        messages,
        temperature: 1.0,
        max_tokens: local_max_tokens,
    };

    // Log the full request (DEBUG level)
    log::debug!("Sending API request: {:?}", req);

    // Build the request but don't send yet
    let request_builder = client
        .post("https://api.x.ai/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&req);

    // Print thinking message
    println!("{}", THINKING_MESSAGE);

    // Now send and await
    let res = request_builder.send().await;

    match res {
        Ok(resp) if resp.status().is_success() => {
            let chat_resp: ChatResponse = resp.json().await.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            let assistant_content = chat_resp.choices[0].message.content.clone();
            if let Some(reason) = &chat_resp.choices[0].finish_reason {
                if reason == "length" {
                    println!("Grok has thought.");
                    println!("Warning: Response truncated due to max_tokens limit!");
                    // Optionally append a note to the chat file: "... [truncated]"
                }
            }
            println!("Received from API: {}", assistant_content);

            // Append to file
            println!("Grok has thought.");
            let mut file = fs::OpenOptions::new().append(true).open(chat_path)?;
            writeln!(
                file,
                "\n{}:\n{}\n\n{}:\n",
                GROK_RESPONSE_MARKER,
                assistant_content,
                USER_PROMPT_MARKER
            )?;

            // Play chime sound
            play_chime().await;

            Ok(())
        }
        Ok(resp) => {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            println!("Grok failed to respond.");
            play_warning().await;
            Err(io::Error::new(io::ErrorKind::Other, format!("API error: {} - Body: {}", status, err_body)))
        }
        Err(e) => {
            println!("Grok failed to respond.");
            play_warning().await;
            Err(io::Error::new(io::ErrorKind::Other, format!("Request error: {:?}", e)))
        },
    }
}

fn parse_chat_messages(content: &str) -> Vec<Message> {
    let mut messages = Vec::new();
    let mut current_role: Option<String> = None;
    let mut current_content = String::new();

    for line in content.lines() {
        if line == "USER PROMPT:" || line == "GROK RESPONSE:" {
            // Add previous section if content is non-empty
            let trimmed = current_content.trim().to_string();
            if !trimmed.is_empty() {
                let role = current_role.take().unwrap_or("user".to_string());
                messages.push(Message {
                    role,
                    content: trimmed,
                });
            }

            // Start new section
            current_role = Some(if line == "USER PROMPT:" { "user".to_string() } else { "assistant".to_string() });
            current_content.clear();
        } else {
            // Append to current content
            writeln!(&mut current_content, "{}", line).expect("Failed to write to String");
        }
    }

    // Add the last section if content is non-empty
    let trimmed = current_content.trim().to_string();
    if !trimmed.is_empty() {
        let role = current_role.unwrap_or("user".to_string());
        messages.push(Message {
            role,
            content: trimmed,
        });
    }

    messages
}

fn expand_placeholders(text: &str) -> io::Result<String> {
    let re = Regex::new(r"@f\s*:(\S+)|@d\s*:(\S+)").unwrap();
    let mut result = String::new();
    let mut last_end = 0;

    for cap in re.captures_iter(text) {
        let match_range = cap.get(0).unwrap();
        let placeholder = match_range.as_str();
        let match_start = match_range.start();
        result.push_str(&text[last_end..match_start]);

        if let Some(file_path) = cap.get(1) {
            let path_str = file_path.as_str();
            match expand_file_path(path_str) {
                Ok(expanded) => result.push_str(&expanded),
                Err(e) => {
                    println!("Warning: Failed to expand file placeholder '{}' : {} (path: {})", placeholder, e, path_str);
                    result.push_str(placeholder);
                }
            }
        } else if let Some(dir_path) = cap.get(2) {
            let path_str = dir_path.as_str();
            match expand_dir_tree(path_str) {
                Ok(expanded) => result.push_str(&expanded),
                Err(e) => {
                    println!("Warning: Failed to expand directory placeholder '{}' : {} (path: {})", placeholder, e, path_str);
                    result.push_str(placeholder);
                }
            }
        }

        last_end = match_range.end();
    }

    result.push_str(&text[last_end..]);
    Ok(result)
}

fn expand_file_path(path_str: &str) -> io::Result<String> {
    let path = Path::new(path_str);
    let mut output = String::new();

    if path_str.contains('*') || path_str.contains('?') {
        // Glob
        let mut files: Vec<_> = glob(path_str)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?
            .filter_map(|res| res.ok().filter(|p| p.is_file()))
            .collect();
        if files.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "No files matched the pattern"));
        }
        files.sort();
        for p in files {
            let content = fs::read_to_string(&p)?;
            writeln!(&mut output, "Contents of {}:\n```\n{}\n```\n", p.display(), content).expect("Failed to write to String");
        }
    } else if path.is_dir() {
        // Directory recurse
        if !path.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "Directory not found"));
        }
        let mut entries: Vec<_> = WalkDir::new(path).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()).collect();
        if entries.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "No files found in directory"));
        }
        entries.sort_by_key(|e| e.path().to_owned());
        for entry in entries {
            let entry_path = entry.path();
            if !entry_path.exists() {
                return Err(io::Error::new(io::ErrorKind::NotFound, format!("File not found in directory: {}", entry_path.display())));
            }
            let content = fs::read_to_string(entry_path)?;
            writeln!(&mut output, "Contents of {}:\n```\n{}\n```\n", entry_path.display(), content).expect("Failed to write to String");
        }
    } else {
        // Single file
        if !path.exists() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "File not found"));
        }
        if !path.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Path is not a file"));
        }
        let content = fs::read_to_string(path)?;
        writeln!(&mut output, "Contents of {}:\n```\n{}\n```\n", path.display(), content).expect("Failed to write to String");
    }

    Ok(output)
}

fn expand_dir_tree(path_str: &str) -> io::Result<String> {
    let path = Path::new(path_str);
    if !path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Directory not found"));
    }
    if !path.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Path is not a directory"));
    }

    let mut output = format!("Contents of directory {}:\n```\n", path.display());
    let mut entries: Vec<_> = WalkDir::new(path).min_depth(1).into_iter().filter_map(|e| e.ok()).collect();
    if entries.is_empty() {
        output.push_str("(empty directory)\n");
    } else {
        entries.sort_by_key(|e| e.path().to_owned());
        for entry in entries {
            let rel_path = entry.path().strip_prefix(path).unwrap();
            let indent = "  ".repeat(entry.depth() - 1);
            if entry.file_type().is_dir() {
                writeln!(&mut output, "{}{}/", indent, rel_path.display()).expect("Failed to write to String");
            } else {
                writeln!(&mut output, "{}{}", indent, rel_path.display()).expect("Failed to write to String");
            }
        }
    }
    output.push_str("```\n");
    Ok(output)
}

// Play a pleasant chime sound from bundled MP3
async fn play_chime() {
    tokio::task::spawn_blocking(|| {
        let (_stream, stream_handle) = OutputStream::try_default().expect("Failed to get default output stream");
        let sink = Sink::try_new(&stream_handle).expect("Failed to create sink");

        // Bundle the MP3 file into the binary
        let bytes = include_bytes!("../media/chime.mp3");
        let cursor = Cursor::new(bytes.as_ref());
        let source = Decoder::new(cursor).expect("Failed to decode MP3");

        sink.append(source);
        sink.sleep_until_end(); // Wait for playback to finish
    })
    .await
    .expect("Failed to play chime");
}

// Play a warning sound (descending tones)
async fn play_warning() {
    tokio::task::spawn_blocking(|| {
        let (_stream, stream_handle) = OutputStream::try_default().expect("Failed to get default output stream");
        let sink = Sink::try_new(&stream_handle).expect("Failed to create sink");

        // Warning: three descending sine waves (e.g., 659Hz, 523Hz, 440Hz for E5, C5, A4 notes)
        let frequencies = [659, 523, 440];
        for freq in frequencies {
            let source = SineWave::new(freq as f32).take_duration(StdDuration::from_millis(200)).amplify(0.20); // Short, soft tone
            sink.append(source);
            std::thread::sleep(StdDuration::from_millis(50)); // Small gap between tones
        }

        sink.sleep_until_end(); // Wait for playback to finish
    })
    .await
    .expect("Failed to play warning");
}
