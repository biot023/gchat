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
const MAX_LEVEL: u32 = 5; // Corresponds to L5 = 16384, as per original default

// Add this constant for the system instructions
const SYSTEM_INSTRUCTIONS: &str = r#"
You are Grok, a helpful AI. If you need the contents of files to better answer the user's query, you can request them by responding with EXACTLY this format and NOTHING ELSE:
GROK REQUESTS FILES: relative/path1, relative/path2
Paths must be relative to the current working directory (e.g., src/main.rs, not /absolute/path or ../outside). Do not request files outside the project directory. You can request multiple files, directories, or globs (e.g., src/*.rs). The system will automatically include their contents in the next user message. Request all needed files at once if possible. You may request again if more are needed after seeing the contents.
"#;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "A utility to communicate with the Grok 4 API via a watched chat file.",
    long_about = r#"
A utility to communicate with the Grok 4 API by watching a Markdown chat file (default: ./gchat.md).
The app polls the file every 1 second for changes. When a new user prompt is detected (marked by "USER PROMPT:"),
it sends the conversation history to the Grok API, appends the response (marked by "GROK RESPONSE:"),
and adds a new "USER PROMPT:" section for your next input. Plays a chime on success or a warning sound on failure.

### Setup
- Set the XAI_API_KEY environment variable with your Grok API key (e.g., export XAI_API_KEY=your_key).
- Optionally, set RUST_LOG for logging (e.g., RUST_LOG=debug for detailed output, including API requests/responses).
- Run the app: cargo run -- [options]. It runs indefinitely until killed (e.g., Ctrl+C).

### Basic Usage
1. Start the app. It creates ./gchat.md if it doesn't exist.
2. Edit ./gchat.md in your text editor:
   - Add your prompt under a "USER PROMPT:" marker.
   - Save the file. The app detects the change, sends it to Grok, and appends the response.
   - Example file content:
     USER PROMPT:
     Hello, Grok!

     GROK RESPONSE:
     Hello! How can I help?

     USER PROMPT:
     What's the weather like? (This will be sent next)
3. The app processes only if the last section is a non-empty "USER PROMPT:".
4. On startup, it processes any pending user prompt in the file.

### Placeholders in User Prompts
These are expanded ONLY in "USER PROMPT:" sections before sending to the API:
- @f :path - Include file contents (e.g., @f :./file.txt). Supports globs (e.g., @f :./*.rs) or directories (recursively includes all files).
- @d :path - Include directory tree listing (e.g., @d :./src). Shows files and subdirs recursively.
- @t :L<level> - Set max_tokens for this prompt (e.g., @t :L3 for 4096 tokens). Removed after processing; last @t in the prompt wins. Levels: L0 (512) to L5 (16384).
- @p :<value> - Set temperature for this prompt (e.g., @p :0.9 for temperature=0.9). Removed after processing; last @p in the prompt wins. Value is a float (e.g., 0.0 to 2.0).

Placeholders are removed/expanded before API calls. Warnings are printed on expansion errors.

### Command-Line Options
- -f/--chat-file: Path to the chat file (default: ./gchat.md).
- -t/--max-tokens: Default max_tokens level (e.g., L5 for 16384; max L5). Overridable per-prompt with @t.
- -p/--temperature: Default temperature (e.g., 1.0). Overridable per-prompt with @p. Must be a float.
- -T/--api-timeout: API request timeout in seconds (default: 600).
- -a/--auto-request-files: Enable Grok to automatically request and include project files (default: false).
- -i/--auto-increase-max-tokens: Automatically increase max_tokens level on truncation (up to L5) by re-querying (default: false).

### Notes
- Requires the 'rodio' crate for sounds (ensure audio dependencies are installed).
- Logs to stderr (configure with RUST_LOG env var).
- If response is truncated (due to max_tokens), a warning is printed.
- Errors (e.g., API failures) play a warning sound and print details.

### File Request Feature (Optional)
Enable with --auto-request-files (-a). When enabled, Grok can request files from the project directory (current working directory) if needed. It responds with "GROK REQUESTS FILES: relative/path1, relative/path2" (exact format). The utility detects this, validates the paths (must be relative and within the project), appends a visible note with @f: placeholders to the last USER PROMPT (e.g., "\n\nGROK REQUESTED FILES:\n@f:src/main.rs\n@f:Cargo.toml\n"), and re-queries the API with the file contents included. This chains until a normal response is received.

The note and placeholders are visible in the chat file (for user awareness) and expanded/included in the next API call (so Grok sees the contents). Requests are handled internally; invalid requests are treated as normal responses.

Examples:
- User prompt: "What's in my project's main file?"
- Grok requests: GROK REQUESTS FILES: src/main.rs
- App appends to USER PROMPT: \n\nGROK REQUESTED FILES:\n@f:src/main.rs\n
- Supports directories/globs if requested (e.g., src/*).

Security: Requests outside the project are ignored. Default: disabled.

### Auto-Increase Max Tokens Feature (Optional)
Enable with --auto-increase-max-tokens (-i). When enabled and a response is truncated (finish_reason: "max_tokens" or "length"), the utility automatically increments the max_tokens level (starting from the prompt's @t or default) and re-queries with the same messages and higher max_tokens (e.g., from L3 to L4). This chains until a non-truncated response is received or L5 (16384 tokens) is reached. If still truncated at L5, the response is appended with a warning.

No changes are made to the chat file until a final (non-truncated or max-level) response is received. Retries are handled in-memory for efficiency. Visible console output shows retry attempts (e.g., "Response truncated. Retrying with L4 (8192 tokens)").

This feature operates independently of --auto-request-files but can chain with it (e.g., a retry might trigger a file request).
"#
)]
struct Args {
    #[arg(short = 'f', long, default_value = "./gchat.md")]
    chat_file: String,

    #[arg(short = 't', long, default_value = "L3")]
    max_tokens: String,

    #[arg(short = 'p', long, default_value = "1.0")]
    temperature: f32,

    #[arg(short = 'm', long, default_value = "grok-4")]  // Default to "grok-4" or "grok-beta" based on testing
    model: String,

    #[arg(long, default_value = "600")]
    api_timeout: u64,

    #[arg(short = 'a', long = "auto-request-files", default_value = "false", help = "Enable Grok to automatically request and include project files (default: false)")]
    auto_request_files: bool,

    #[arg(short = 'i', long = "auto-increase-max-tokens", default_value = "false", help = "Automatically increase max_tokens on truncation (default: false)")]
    auto_increase_max_tokens: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

    // Parse the default level and max_tokens
    let default_level = match get_level_from_str(&args.max_tokens) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing --max-tokens: {}", e);
            std::process::exit(1);
        }
    };
    let default_max_tokens = 512u32 << default_level;

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
    println!("  Max tokens: {} ({})", args.max_tokens, default_max_tokens);
    println!("  Temperature: {}", args.temperature);
    println!("  API model: {}", args.model);
    println!("  API timeout: {} seconds", args.api_timeout);
    println!("  Auto request files: {}", args.auto_request_files);
    println!("  Auto increase max tokens: {}", args.auto_increase_max_tokens);

    println!("App started. Polling {} for changes every 1 second.", args.chat_file);

    // Initial process on startup
    if let Err(e) = process_chat_file(
        &chat_path,
        default_level,
        args.temperature,
        args.api_timeout,
        args.auto_request_files,
        args.auto_increase_max_tokens,
        &args.model,
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
                default_level,
                args.temperature,
                args.api_timeout,
                args.auto_request_files,
                args.auto_increase_max_tokens,
                &args.model,
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

fn get_level_from_str(s: &str) -> Result<u32, String> {
    let s = s.trim();
    if let Some(lstr) = s.strip_prefix('L') {
        match lstr.parse::<u32>() {
            Ok(level) if level <= MAX_LEVEL => Ok(level),
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

fn parse_level(level: u32) -> u32 {
    512u32 << level
}

async fn process_chat_file(
    chat_path: &PathBuf,
    default_level: u32,
    default_temperature: f32,
    api_timeout: u64,
    auto_request_files: bool,
    auto_increase_max_tokens: bool,
    model: &str,
) -> io::Result<()> {
    // Short debounce to ensure save is complete (helps with atomic saves)
    sleep(Duration::from_millis(500)).await;

    // Outer loop to handle chained file requests (which modify the file)
    loop {
        let content = fs::read_to_string(chat_path)?;
        let mut messages = parse_chat_messages(&content);

        if messages.is_empty() || messages.last().unwrap().role != "user" || messages.last().unwrap().content.trim().is_empty() {
            println!("No complete user prompt to process in chat file.");
            return Ok(()); // No send needed
        }

        // Handle @t placeholders: remove from all user messages, and track the last @t across all user messages
        let re_t = Regex::new(r"@t\s*:\s*L(\d+)").unwrap();
        let mut persistent_level: Option<u32> = None;
        for i in 0..messages.len() {
            if messages[i].role == "user" {
                let content = &messages[i].content;
                let mut new_content = content.to_string();
                let mut last_level: Option<u32> = None;
                let mut ranges = vec![];
                for cap in re_t.captures_iter(content) {
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
                // Update persistent_level if this message had a @t
                if let Some(lvl) = last_level {
                    persistent_level = Some(lvl);
                }
            }
        }

        // Set current_level based on persistent or default, with capping if needed
        let mut current_level = default_level;
        if let Some(lvl) = persistent_level {
            current_level = lvl;
            if current_level > MAX_LEVEL {
                println!(
                    "Warning: Specified level L{} too high, capping at L{} ({} tokens)",
                    lvl,
                    MAX_LEVEL,
                    512u32 << MAX_LEVEL
                );
                current_level = MAX_LEVEL;
            }
            println!("Setting `max_tokens` API parameter to {}", parse_level(current_level));
        }

        // Handle @p placeholders: similar to @t, remove from all user messages, track the last @p across all user messages
        let mut local_temperature = default_temperature;
        let re_p = Regex::new(r"@p\s*:\s*(\d*\.?\d+)").unwrap();
        let mut persistent_temperature: Option<f32> = None;
        for i in 0..messages.len() {
            if messages[i].role == "user" {
                let content = &messages[i].content;
                let mut new_content = content.to_string();
                let mut last_temp: Option<f32> = None;
                let mut ranges = vec![];
                for cap in re_p.captures_iter(content) {
                    let whole = cap.get(0).unwrap();
                    ranges.push(whole.range());
                    if let Some(num_str) = cap.get(1) {
                        if let Ok(temp) = num_str.as_str().parse::<f32>() {
                            last_temp = Some(temp);
                        }
                    }
                }
                // Remove in reverse order to avoid index issues
                for range in ranges.into_iter().rev() {
                    new_content.replace_range(range, "");
                }
                messages[i].content = new_content;
                // Update persistent_temperature if this message had a @p
                if let Some(temp) = last_temp {
                    persistent_temperature = Some(temp);
                }
            }
        }
        // After processing all messages, apply the last seen temperature if any
        if let Some(temp) = persistent_temperature {
            local_temperature = temp;
            // Optional: Clamp to reasonable range (e.g., 0.0 to 2.0)
            if local_temperature < 0.0 || local_temperature > 2.0 {
                println!(
                    "Warning: Specified temperature {} is outside typical range (0.0-2.0), using as-is.",
                    local_temperature
                );
            }
            println!("Setting `temperature` API parameter to {}", local_temperature);
        }

        // Expand other placeholders ONLY in user messages (prompts to the API)
        for msg in messages.iter_mut() {
            if msg.role == "user" {
                msg.content = expand_placeholders(&msg.content)?;
            }
        }

        // Log the expanded messages (DEBUG level)
        log::debug!("Expanded messages for API request: {:?}", messages);

        // Prepend system instructions ONLY if flag is enabled
        let mut api_messages = messages.clone();  // Clone to avoid mutating original
        if auto_request_files {
            api_messages.insert(0, Message {
                role: "system".to_string(),
                content: SYSTEM_INSTRUCTIONS.to_string(),
            });
        }

        // Get API key, build client
        let api_key = env::var("XAI_API_KEY").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "XAI_API_KEY not set"))?;
        let client = Client::builder()
            .timeout(Duration::from_secs(api_timeout))
            .build()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Inner loop for handling truncation retries (in-memory, no file re-read)
        let mut needs_reprocess = false;
        loop {
            // Create request with current max_tokens
            let req = ChatRequest {
                model: model.to_string(),
                messages: api_messages.clone(),  // Clone to keep immutable
                temperature: local_temperature,
                max_tokens: parse_level(current_level),
            };

            // Log the full request (DEBUG level)
            log::debug!("Sending API request: {:?}", req);

            // Build the request
            let request_builder = client
                .post("https://api.x.ai/v1/chat/completions")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&req);

            // Print thinking message with settings
            println!("Grok is thinking... (max_tokens: {}, temperature: {})", req.max_tokens, local_temperature);

            // Send and await
            let res = request_builder.send().await;

            match res {
                Ok(resp) if resp.status().is_success() => {
                    let chat_resp: ChatResponse = resp.json().await.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    let assistant_content = chat_resp.choices[0].message.content.clone();
                    let finish_reason = chat_resp.choices[0].finish_reason.clone();

                    // Check if this is a file request (only if flag is enabled)
                    let mut is_file_request = false;
                    if auto_request_files {
                        let trimmed = assistant_content.trim();
                        if trimmed.starts_with("GROK REQUESTS FILES:") {
                            let rest = trimmed.strip_prefix("GROK REQUESTS FILES:").unwrap().trim();
                            // Ensure it's exactly the format (no extra content)
                            if !rest.is_empty() && trimmed == format!("GROK REQUESTS FILES: {}", rest) {
                                let paths: Vec<String> = rest.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

                                // Validate paths
                                let cwd = env::current_dir().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
                                let mut all_valid = true;
                                let mut valid_paths = vec![];
                                for p in paths.iter() {
                                    let path = PathBuf::from(p);
                                    // Block absolute paths or parent traversal
                                    if path.is_absolute() || p.starts_with("..") || p.contains("..") {
                                        println!("Warning: Invalid path requested (traversal attempt): {}", p);
                                        all_valid = false;
                                        break;
                                    }
                                    // Canonicalize and check if within cwd
                                    let full_path = cwd.join(&path);
                                    match full_path.canonicalize() {
                                        Ok(canon) if canon.starts_with(&cwd) => {
                                            valid_paths.push(p.clone());
                                        }
                                        _ => {
                                            println!("Warning: Path outside project or invalid: {}", p);
                                            all_valid = false;
                                            break;
                                        }
                                    }
                                }

                                if all_valid && !valid_paths.is_empty() {
                                    // Append visible note and placeholders to the END of the file (augments the last USER PROMPT)
                                    let mut file = fs::OpenOptions::new().append(true).open(chat_path)?;
                                    writeln!(file, "\n\nGROK REQUESTED FILES:")?;
                                    for vp in valid_paths {
                                        writeln!(file, "@f:{}", vp)?;  // No space after 'f'
                                    }

                                    // Set flag to reprocess (re-read file) and break inner loop
                                    is_file_request = true;
                                    needs_reprocess = true;
                                }
                            }
                        }
                    }

                    // If it was a valid file request, break inner loop to allow outer loop to re-read
                    if is_file_request {
                        break;
                    }

                    // Check for truncation
                    let is_truncated = finish_reason.as_ref().map(|r| r == "max_tokens" || r == "length").unwrap_or(false);
                    if auto_increase_max_tokens && is_truncated && current_level < MAX_LEVEL {
                        current_level += 1;
                        println!(
                            "Response truncated. Retrying with higher max_tokens: L{} ({} tokens)",
                            current_level, parse_level(current_level)
                        );
                        // Continue inner loop to re-query with higher max_tokens
                        continue;
                    }

                    // Otherwise, treat as final response
                    println!("Grok has thought.");
                    let mut file = fs::OpenOptions::new().append(true).open(chat_path)?;
                    writeln!(
                        file,
                        "\n{}:\n{}\n\n{}:\n",
                        GROK_RESPONSE_MARKER,
                        assistant_content,
                        USER_PROMPT_MARKER
                    )?;

                    // If still truncated at max level, print warning
                    if is_truncated {
                        println!("Warning: Response truncated even at max level L{} ({} tokens)!", MAX_LEVEL, parse_level(MAX_LEVEL));
                    }

                    // Play chime sound
                    play_chime().await;

                    // Break inner loop after handling final response
                    break;
                }
                Ok(resp) => {
                    let status = resp.status();
                    let err_body = resp.text().await.unwrap_or_default();
                    println!("Grok failed to respond.");
                    play_warning().await;
                    return Err(io::Error::new(io::ErrorKind::Other, format!("API error: {} - Body: {}", status, err_body)));
                }
                Err(e) => {
                    println!("Grok failed to respond.");
                    play_warning().await;
                    return Err(io::Error::new(io::ErrorKind::Other, format!("Request error: {:?}", e)));
                },
            }
        }  // End inner loop

        // After inner loop, check if we need to reprocess (e.g., for file requests)
        if !needs_reprocess {
            break;  // Done processing, break outer loop
        }
        // Else, continue outer loop to re-read the updated file
    }  // End outer loop

    Ok(())
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
