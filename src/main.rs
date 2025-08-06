use clap::Parser;
use notify::{recommended_watcher, RecursiveMode, Watcher, Event};
use notify::Result as NotifyResult;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File};
use std::io::{self, Write as IoWrite};
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc::{channel, Receiver}};
use std::time::Duration;
use tokio::time::sleep;
use walkdir::WalkDir;
use glob::glob;
use rodio::{OutputStream, Sink, Source, source::SineWave};
use std::time::Duration as StdDuration;

const GROK_RESPONSE_MARKER: &str = "GROK RESPONSE";
const USER_PROMPT_MARKER: &str = "USER PROMPT";
const THINKING_MESSAGE: &str = "Grok is thinking...";

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short = 'f', long, default_value = "./gchat.md")]
    chat_file: String,

    #[arg(short = 't', long, default_value = "4096")]
    max_tokens: u32,

    #[arg(short = 'T', long, default_value = "600")]
    api_timeout: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize)]
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
    let args = Args::parse();
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
    println!("  Max tokens: {}", args.max_tokens);
    println!("  API timeout: {} seconds", args.api_timeout);

    let ignoring_next_change = Arc::new(Mutex::new(false));

    let ignoring_clone = ignoring_next_change.clone();
    let chat_path_clone = chat_path.clone();

    println!("App started. Watching {} for changes.", args.chat_file);

    // Initial process on startup
    if let Err(e) = process_chat_file(
        &chat_path_clone,
        args.max_tokens,
        args.api_timeout,
        &ignoring_clone,
    )
    .await
    {
        println!("Processing error: {}", e);
    }

    // Set up watcher
    let (tx, rx): (std::sync::mpsc::Sender<NotifyResult<Event>>, Receiver<NotifyResult<Event>>) = channel();
    let mut watcher = recommended_watcher(move |res| { let _ = tx.send(res); })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    watcher.watch(&chat_path_clone, RecursiveMode::NonRecursive)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    loop {
        if let Ok(res) = rx.recv() {
            match res {
                Ok(event) if event.kind.is_modify() => {
                    // Debounce
                    sleep(Duration::from_millis(500)).await;

                    // Check if ignoring
                    let mut ignore = ignoring_clone.lock().unwrap();
                    if *ignore {
                        *ignore = false;
                        continue;
                    }

                    if let Err(e) = process_chat_file(
                        &chat_path_clone,
                        args.max_tokens,
                        args.api_timeout,
                        &ignoring_clone,
                    )
                    .await
                    {
                        println!("Processing error: {}", e);
                    }
                }
                Ok(_) => {}, // Ignore other kinds
                Err(e) => println!("Watcher error: {}", e),
            }
        } else {
            break; // Channel closed
        }
    }

    Ok(())
}

async fn process_chat_file(
    chat_path: &PathBuf,
    max_tokens: u32,
    api_timeout: u64,
    ignoring_next_change: &Arc<Mutex<bool>>,
) -> io::Result<()> {
    let content = fs::read_to_string(chat_path)?;
    let mut messages = parse_chat_messages(&content);

    println!("Parsed messages: {:?}", messages);

    if messages.is_empty() || messages.last().unwrap().role != "user" {
        println!("No user prompt to process in chat file.");
        return Ok(()); // No send needed
    }

    // Expand placeholders ONLY in user messages (prompts to the API)
    for msg in messages.iter_mut() {
        if msg.role == "user" {
            msg.content = expand_placeholders(&msg.content)?;
        }
    }

    println!("Sending to API: {:?}", messages);

    // Get API key, build client, create req (unchanged)
    let api_key = env::var("XAI_API_KEY").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "XAI_API_KEY not set"))?;
    let client = Client::builder()
        .timeout(Duration::from_secs(api_timeout))
        .build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    let req = ChatRequest {
        model: "grok-4-0709".to_string(),
        messages,
        temperature: 1.0,
        max_tokens,
    };

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

            // Set ignore flag
            *ignoring_next_change.lock().unwrap() = true;

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
    let grok_marker_with_newlines = format!("\n{}:\n", GROK_RESPONSE_MARKER);
    let user_marker_with_newlines = format!("\n{}:\n", USER_PROMPT_MARKER);

    let parts: Vec<&str> = content.split(&grok_marker_with_newlines).collect();

    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                messages.push(Message { role: "user".to_string(), content: trimmed.to_string() });
            }
        } else {
            let subparts: Vec<&str> = part.split(&user_marker_with_newlines).collect();
            for (j, sub) in subparts.iter().enumerate() {
                let trimmed = sub.trim();
                if !trimmed.is_empty() {
                    let role = if j == 0 { "assistant" } else { "user" };
                    messages.push(Message { role: role.to_string(), content: trimmed.to_string() });
                }
            }
        }
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
            let expanded = expand_file_path(path_str)
                .map_err(|e| io::Error::new(e.kind(), format!("Error: Failed to expand file placeholder '{}': {} (path: {})", placeholder, e, path_str)))?;
            result.push_str(&expanded);
        } else if let Some(dir_path) = cap.get(2) {
            let path_str = dir_path.as_str();
            let expanded = expand_dir_tree(path_str)
                .map_err(|e| io::Error::new(e.kind(), format!("Error: Failed to expand directory placeholder '{}': {} (path: {})", placeholder, e, path_str)))?;
            result.push_str(&expanded);
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
        let mut paths: Vec<_> = glob(path_str).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?.filter_map(Result::ok).collect();
        if paths.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "No files matched the glob pattern"));
        }
        paths.sort();
        for p in paths {
            if !p.exists() {
                return Err(io::Error::new(io::ErrorKind::NotFound, format!("File not found: {}", p.display())));
            }
            if p.is_file() {
                let content = fs::read_to_string(&p)?;
                writeln!(&mut output, "Contents of {}:\n```\n{}\n```\n", p.display(), content).expect("Failed to write to String");
            }
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

// Play a pleasant chime sound (ascending tones)
async fn play_chime() {
    tokio::task::spawn_blocking(|| {
        let (_stream, stream_handle) = OutputStream::try_default().expect("Failed to get default output stream");
        let sink = Sink::try_new(&stream_handle).expect("Failed to create sink");

        // Chime: three ascending sine waves (e.g., 440Hz, 523Hz, 659Hz for A4, C5, E5 notes)
        let frequencies = [440, 523, 659];
        for freq in frequencies {
            let source = SineWave::new(freq as f32).take_duration(StdDuration::from_millis(200)).amplify(0.20); // Short, soft tone
            sink.append(source);
            std::thread::sleep(StdDuration::from_millis(50)); // Small gap between tones
        }

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
