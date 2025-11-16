use clap::{Arg, Command};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File};
use std::io::{self, Write as IoWrite};
use std::fmt::Write as FmtWrite;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use std::path::Component;
use tokio::time::sleep;
use walkdir::WalkDir;
use glob::glob;
use rodio::{OutputStream, Sink, Source, source::SineWave, Decoder};
use std::time::Duration as StdDuration;
use std::io::Cursor;
use log;
use dirs;
use toml;
use shell_words::split; // Added for safe RG and FD command parsing
use std::collections::HashMap; // NEW: For profiles

const GROK_RESPONSE_MARKER: &str = "GROK RESPONSE";
const USER_PROMPT_MARKER: &str = "USER PROMPT";
const MAX_LEVEL: u32 = 12; // UPDATED: Increased from 7 to 12 for ~2M tokens
const RG_TIMEOUT_SECS: u64 = 30; // Added for RG command timeout
const MAX_RG_OUTPUT_BYTES: usize = 50 * 1024; // Added for RG output limit
const FD_TIMEOUT_SECS: u64 = 30; // Added for FD command timeout
const MAX_FD_OUTPUT_BYTES: usize = 50 * 1024; // Added for FD output limit

fn get_system_instructions(provider: &str) -> &'static str {
    match provider {
        "claude" => r#"
You are Claude, a helpful AI and coding assistant. **To provide the most accurate and helpful responses, actively request the contents of relevant files whenever you need them to verify assumptions, check details, or gather more context—even if the user hasn't explicitly asked. For example, if a query involves code, configurations, or project structure, request the necessary files proactively.**

If you decide to request files, respond with **EXACTLY** this format and **NOTHING ELSE**:
CLAUDE REQUESTS FILES: relative/path1, relative/path2
Paths must be relative to the current working directory (e.g., src/main.rs, not /absolute/path or ../outside). Do not request files outside the project directory. You can request multiple files, directories, or globs (e.g., src/*.rs). The system will automatically include their contents in the next user message. Request all needed files at once if possible. You may request again if more are needed after seeing the contents.

**Only request files when they are genuinely needed to improve your response. If you have sufficient information, provide a direct answer without requesting.**

To perform grep-like searches on the project, respond with **EXACTLY** this format and **NOTHING ELSE** (chaining until done):
CLAUDE RUNS RG: rg <safe-args-and-patterns>
Examples: CLAUDE RUNS RG: rg -i "error" --glob "**/*.rs" --line-number
Use --glob for patterns (e.g., --glob "**/*.rs" for all Rust files recursively). Avoid bare globs like src/*.rs without --glob. Allowed args: common ripgrep flags like -i, -n, --type rust, paths (relative only). No execution or shell metacharacters.

To search for files and directories on the project, respond with **EXACTLY** this format and **NOTHING ELSE** (chaining until done):
CLAUDE RUNS FD: fd <safe-args-and-patterns>
Examples: CLAUDE RUNS FD: fd --type f --glob "*.md" --max-depth 2
Allowed args: common fd flags like --type, --glob, --max-depth, paths (relative only). No execution or shell metacharacters.
"#,
        _ => r#"
You are Grok, a helpful AI and coding assistant. **To provide the most accurate and helpful responses, actively request the contents of relevant files whenever you need them to verify assumptions, check details, or gather more context—even if the user hasn't explicitly asked. For example, if a query involves code, configurations, or project structure, request the necessary files proactively.**

If you decide to request files, respond with **EXACTLY** this format and **NOTHING ELSE**:
GROK REQUESTS FILES: relative/path1, relative/path2
Paths must be relative to the current working directory (e.g., src/main.rs, not /absolute/path or ../outside). Do not request files outside the project directory. You can request multiple files, directories, or globs (e.g., src/*.rs). The system will automatically include their contents in the next user message. Request all needed files at once if possible. You may request again if more are needed after seeing the contents.

**Only request files when they are genuinely needed to improve your response. If you have sufficient information, provide a direct answer without requesting.**

To perform grep-like searches on the project, respond with **EXACTLY** this format and **NOTHING ELSE** (chaining until done):
GROK RUNS RG: rg <safe-args-and-patterns>
Examples: GROK RUNS RG: rg -i "error" --glob "**/*.rs" --line-number
Use --glob for patterns (e.g., --glob "**/*.rs" for all Rust files recursively). Avoid bare globs like src/*.rs without --glob. Allowed args: common ripgrep flags like -i, -n, --type rust, paths (relative only). No execution or shell metacharacters.

To search for files and directories on the project, respond with **EXACTLY** this format and **NOTHING ELSE** (chaining until done):
GROK RUNS FD: fd <safe-args-and-patterns>
Examples: GROK RUNS FD: fd --type f --glob "*.md" --max-depth 2
Allowed args: common fd flags like --type, --glob, --max-depth, paths (relative only). No execution or shell metacharacters.
"#,
    }
}

fn get_write_instructions(provider: &str) -> &'static str {
    match provider {
        "claude" => r#"
To write file contents, if the user prompt contains a placeholder like `@w:relative/path` (indicating they want you to generate and provide the full content for that file), respond with **EXACTLY** this format and **NOTHING ELSE**:
CLAUDE WRITES TO FILE: relative/path
[full exact content here, with no markdown formatting, code blocks, or extra explanations— just the raw content to write to the file]

The path must exactly match the relative path specified in the `@w:path` placeholder from the user's current prompt (case-sensitive, no modifications). For example, if the user says `@w:src/main.rs`, only use that exact path—do not invent or alter paths.

Paths must be relative to the current working directory (e.g., README.md or src/main.rs, not /absolute/path or ../outside). Do not request writes outside the project directory. The system will validate and write the content safely. Only use this if the user explicitly requests writing to a specific file via the `@w:` placeholder, and only for the exact path they specified.
**Only respond in this format when genuinely needed to fulfill a write request. Otherwise, provide a normal response."#,
        _ => r#"
To write file contents, if the user prompt contains a placeholder like `@w:relative/path` (indicating they want you to generate and provide the full content for that file), respond with **EXACTLY** this format and **NOTHING ELSE**:
GROK WRITES TO FILE: relative/path
[full exact content here, with no markdown formatting, code blocks, or extra explanations— just the raw content to write to the file]

The path must exactly match the relative path specified in the `@w:path` placeholder from the user's current prompt (case-sensitive, no modifications). For example, if the user says `@w:src/main.rs`, only use that exact path—do not invent or alter paths.

Paths must be relative to the current working directory (e.g., README.md or src/main.rs, not /absolute/path or ../outside). Do not request writes outside the project directory. The system will validate and write the content safely. Only use this if the user explicitly requests writing to a specific file via the `@w:` placeholder, and only for the exact path they specified.
**Only respond in this format when genuinely needed to fulfill a write request. Otherwise, provide a normal response."#,
    }
}

const DEFAULT_CHAT_FILE: &str = "./gchat.md";
const DEFAULT_MAX_TOKENS: &str = "L3";
const DEFAULT_TEMPERATURE: &str = "1.0";
const DEFAULT_MODEL: &str = "grok-code-fast-1";
const DEFAULT_PROVIDER: &str = "grok";
const DEFAULT_API_TIMEOUT: &str = "600";

fn contains_traversal(p: &str) -> bool {
    Path::new(p).components().any(|c| matches!(c, Component::ParentDir))
}

#[derive(Deserialize, Debug, Clone)]
struct Config {
    chat_file: Option<String>,
    max_tokens: Option<String>,
    temperature: Option<f32>,
    model: Option<String>,
    provider: Option<String>,
    api_timeout: Option<u64>,
    #[serde(default)]
    auto_request_files: bool,
    #[serde(default)]
    auto_increase_max_tokens: bool,
    #[serde(default)]
    allow_rg_commands: bool,
    #[serde(default)]
    allow_fd_commands: bool,
    #[serde(default)]
    allow_file_writes: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Message {
    role: String,
    content: String,
}

#[derive(Serialize, Debug)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Serialize, Debug)]
struct ClaudeRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
    system: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct ClaudeContent {
    text: String,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    env_logger::Builder::from_default_env()
        .format(|buf, record| write!(buf, "{}", record.args()))
        .init();

    // UPDATED: CLI app with profile arg and temperature short changed to 'P'
let app = Command::new("gchat")
    .version(env!("CARGO_PKG_VERSION"))
    .about("Chat with Grok effortlessly! A friendly Rust tool for interactive conversations via a Markdown file.")
    .long_about(
        "Hey there! 🚀 This is gchat, your handy helper for chatting with Grok 4 (from xAI) right from your favorite text editor.\n\n".to_string() +
        "Just edit a Markdown file (like ./gchat.md), add your questions, and watch it magically turn into full conversations. It's perfect for developers, writers, or anyone who loves file-based workflows.\n\n" +
        "Features include:\n" +
        "  - File watching: No need to switch windows—poll for changes every second.\n" +
        "  - Smart placeholders: Include code or files with @f: or @d: (e.g., @f:src/main.rs).\n" +
        "  - Audio vibes: Chime on success, warning tones if things go sideways.\n" +
        "  - Optional superpowers: Auto-request files, run safe searches (RG/FD), or even let Grok edit your project files!\n\n" +
        "Get started: Run gchat, edit ./gchat.md, and add 'USER PROMPT: Hello, Grok!'. Happy chatting! 🤖✨"
    )
        .arg(
            Arg::new("chat_file")
                .short('f')
                .long("chat-file")
                .value_name("PATH")
                .help("Path to the chat file"),
        )
        .arg(
            Arg::new("max_tokens")
                .short('t')
                .long("max_tokens")
                .value_name("LEVEL")
                .help("Default max tokens level"),
        )
        .arg(
            Arg::new("temperature")  // UPDATED: Changed short from 'p' to 'P'
                .short('P')
                .long("temperature")
                .value_name("FLOAT")
                .help("Default temperature"),
        )
        .arg(
            Arg::new("model")
                .short('m')
                .long("model")
                .value_name("STRING")
                .help("The AI model to call"),
        )
        .arg(
            Arg::new("provider")
                .long("provider")
                .value_name("STRING")
                .help("AI provider: 'grok' or 'claude'"),
        )
        .arg(
            Arg::new("api_timeout")
                .long("api-timeout")
                .value_name("SECONDS")
                .help("API request timeout"),
        )
        .arg(
            Arg::new("auto_request_files")
                .short('a')
                .long("auto-request-files")
                .help("Enable Grok to automatically request and include project files")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("auto_increase_max_tokens")
                .short('i')
                .long("auto-increase-max-tokens")
                .help("Automatically increase max_tokens on truncation")
                .action(clap::ArgAction::SetTrue),
        )
        .arg( // Added
            Arg::new("allow_rg_commands")
                .short('r')
                .long("allow-rg-commands")
                .help("Allow Grok to run safe ripgrep commands on the project")
                .action(clap::ArgAction::SetTrue),
        )
        .arg( // Added
            Arg::new("allow_fd_commands")
                .short('d')
                .long("allow-fd-commands")
                .help("Allow Grok to run safe fd commands on the project")
                .action(clap::ArgAction::SetTrue),
        )
        .arg( // NEW
            Arg::new("allow_file_writes")
                .short('w')
                .long("allow-file-writes")
                .help("Allow Grok to write generated content to project files via special responses")
                .action(clap::ArgAction::SetTrue),
        )
        .arg( // NEW: Profile arg
            Arg::new("profile")
                .short('p')
                .long("profile")
                .value_name("NAME")
                .help("Profile name from config.toml (e.g., 'default' or 'x')"),
        );

    let matches = app.get_matches();

    // UPDATED: Profile-aware config loading
    let mut config: Config = Config {
        chat_file: None,
        max_tokens: None,
        temperature: None,
        model: None,
        provider: None,
        api_timeout: None,
        auto_request_files: false,
        auto_increase_max_tokens: false,
        allow_rg_commands: false,
        allow_fd_commands: false,
        allow_file_writes: false,
    };

    let mut config_loaded = false;
    if let Some(config_dir) = dirs::config_dir() {
        let config_path = config_dir.join("gchat/config.toml");
        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(config_content) => {
                    // Try to parse as profile table (HashMap<String, Config>)
                    match toml::from_str::<HashMap<String, Config>>(&config_content) {
                        Ok(config_table) => {
                            if !config_table.is_empty() {
                                // Select profile
                                let profile_name = matches.get_one::<String>("profile").cloned();
                                let selected = if let Some(name) = &profile_name {
                                    // Use specified profile if exists
                                    if let Some(selected_config) = config_table.get(name) {
                                        println!("Loaded profile '{}' from {}", name, config_path.display());
                                        selected_config.clone()
                                    } else {
                                        // Fallback: try "default", then first
                                        eprintln!("Profile '{}' not found. Falling back to 'default' or first profile.", name);
                                        if let Some(default_config) = config_table.get("default") {
                                            println!("Using 'default' profile from {}", config_path.display());
                                            default_config.clone()
                                        } else if let Some(first) = config_table.values().next() {
                                            let first_key = config_table.keys().next().unwrap();  // For logging
                                            println!("Using first profile '{}' from {}", first_key, config_path.display());
                                            first.clone()
                                        } else {
                                            config.clone()  // Pure defaults
                                        }
                                    }
                                } else {
                                    // No profile specified: prioritize "default", else first
                                    if let Some(default_config) = config_table.get("default") {
                                        println!("Using 'default' profile from {}", config_path.display());
                                        default_config.clone()
                                    } else if let Some(first) = config_table.values().next() {
                                        let first_key = config_table.keys().next().unwrap();  // For logging
                                        println!("Using first profile '{}' from {}", first_key, config_path.display());
                                        first.clone()
                                    } else {
                                        config.clone()  // Pure defaults
                                    }
                                };

                                config = selected;
                                config_loaded = true;
                            }
                        }
                        Err(e) => {
                            // Fallback: Try legacy single-config parse (for backward compat)
                            match toml::from_str::<Config>(&config_content) {
                                Ok(legacy_config) => {
                                    config = legacy_config;
                                    config_loaded = true;
                                    println!("Loaded legacy single config from {}", config_path.display());
                                }
                                Err(_) => {
                                    eprintln!("Error parsing config file {} (tried profiles and legacy): {}", config_path.display(), e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error reading config file {}: {}", config_path.display(), e);
                }
            }
        } else {
            println!("No config file found at {}", config_path.display());
        }
    }

    if !config_loaded {
        println!("No config loaded; using defaults.");
    }

    // Extract final values: CLI overrides config overrides defaults
    let chat_file = if matches.contains_id("chat_file") {
        matches.get_one::<String>("chat_file").unwrap().clone()
    } else {
        config.chat_file.unwrap_or(DEFAULT_CHAT_FILE.to_string())
    };

    let max_tokens_str = if matches.contains_id("max_tokens") {
        matches.get_one::<String>("max_tokens").unwrap().clone()
    } else {
        config.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS.to_string())
    };

    let temperature = if matches.contains_id("temperature") {  // ID unchanged
        matches.get_one::<String>("temperature").unwrap().parse::<f32>().unwrap_or(1.0)
    } else {
        config.temperature.unwrap_or(DEFAULT_TEMPERATURE.parse::<f32>().unwrap())
    };

    let provider = if matches.contains_id("provider") {
        matches.get_one::<String>("provider").unwrap().clone()
    } else {
        config.provider.unwrap_or(DEFAULT_PROVIDER.to_string())
    };

    let model = if matches.contains_id("model") {
        matches.get_one::<String>("model").unwrap().clone()
    } else {
        config.model.unwrap_or_else(|| {
            match provider.as_str() {
                "claude" => "claude-3-5-sonnet-20241022".to_string(),
                _ => DEFAULT_MODEL.to_string(),
            }
        })
    };

    let api_timeout = if matches.contains_id("api_timeout") {
        matches.get_one::<String>("api_timeout").unwrap().parse::<u64>().unwrap_or(600)
    } else {
        config.api_timeout.unwrap_or(DEFAULT_API_TIMEOUT.parse::<u64>().unwrap())
    };

    let auto_request_files = if let Some(true) = matches.get_one::<bool>("auto_request_files") {
        true
    } else {
        config.auto_request_files
    };

    let auto_increase_max_tokens = if let Some(true) = matches.get_one::<bool>("auto_increase_max_tokens") {
        true
    } else {
        config.auto_increase_max_tokens
    };

    let allow_rg_commands = if let Some(true) = matches.get_one::<bool>("allow_rg_commands") {
        true
    } else {
        config.allow_rg_commands
    };

    let allow_fd_commands = if let Some(true) = matches.get_one::<bool>("allow_fd_commands") {
        true
    } else {
        config.allow_fd_commands
    };

    let allow_file_writes = if let Some(true) = matches.get_one::<bool>("allow_file_writes") {
        true
    } else {
        config.allow_file_writes
    };

    // Parse the default level and max_tokens (using the final max_tokens_str)
    let default_level = match get_level_from_str(&max_tokens_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing max_tokens: {}", e);
            std::process::exit(1);
        }
    };
    let default_max_tokens = 512u32 << default_level;

    let chat_path = PathBuf::from(&chat_file);

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
    println!("  Chat file: {}", chat_file);
    println!("  Provider: {}", provider);
    println!("  Max tokens: {} ({})", max_tokens_str, default_max_tokens);
    println!("  Temperature: {}", temperature);
    println!("  API model: {}", model);
    println!("  API timeout: {} seconds", api_timeout);
    println!("  Auto request files: {}", auto_request_files);
    println!("  Auto increase max tokens: {}", auto_increase_max_tokens);
    println!("  Allow RG commands: {}", allow_rg_commands); // Added
    println!("  Allow FD commands: {}", allow_fd_commands); // Added
    println!("  Allow file writes: {}", allow_file_writes); // NEW

    println!("App started. Polling {} for changes every 1 second.", chat_file);

    // Initial process on startup
    if let Err(e) = process_chat_file(
        &chat_path,
        default_level,
        temperature,
        api_timeout,
        auto_request_files,
        auto_increase_max_tokens,
        allow_rg_commands, // Added
        allow_fd_commands, // Added
        allow_file_writes, // NEW
        &model,
        &provider,
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
                temperature,
                api_timeout,
                auto_request_files,
                auto_increase_max_tokens,
                allow_rg_commands, // Added
                allow_fd_commands, // Added
                allow_file_writes, // NEW
                &model,
                &provider,
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
            Err(_) => Err("Invalid level: expected L followed by a number (e.g., L7)".to_string()),
        }
    } else {
        Err("Invalid format: expected L<level> (e.g., L7)".to_string())
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
    allow_rg_commands: bool, // Added
    allow_fd_commands: bool, // Added
    allow_file_writes: bool, // NEW
    model: &str,
    provider: &str,
) -> io::Result<()> {
    // Short debounce to ensure save is complete (helps with atomic saves)
    sleep(Duration::from_millis(500)).await;

    // Outer loop to handle chained file requests (which modify the file)
    loop {
        let content = fs::read_to_string(chat_path)?;
        let mut messages = parse_chat_messages(&content);

        if messages.is_empty() || messages.last().map_or(true, |m| m.role != "user" || m.content.trim().is_empty()) {
            log::debug!("Skipping process: last section is not a non-empty user prompt");
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

        // NEW: Extract @w paths from raw last user content (before any expansion/removal)
        let raw_last_user = messages.last().and_then(|m| if m.role == "user" { Some(&m.content) } else { None }).map(|s| s.as_str()).unwrap_or("");
        let re_w = Regex::new(r#"@w\s*:\s*([^\s<>"'`]+)"#).unwrap();  // Stricter regex
        let mut allowed_write_paths: Vec<String> = re_w
            .captures_iter(raw_last_user)
            .map(|cap| cap.get(1).unwrap().as_str().trim().to_string())
            .collect();
        allowed_write_paths.sort();
        allowed_write_paths.dedup();
        log::debug!("Allowed write paths from raw last user prompt: {:?}", allowed_write_paths);

        // Expand other placeholders ONLY in user messages (prompts to the API)
        let mut any_expansion_error = false;
        let mut all_failed_paths = Vec::new(); // New: Collect all failed paths across messages
        for msg in messages.iter_mut() {
            if msg.role == "user" {
                let (expanded, had_error, failed_paths) = expand_placeholders(&msg.content)?;
                msg.content = expanded;
                any_expansion_error |= had_error;
                all_failed_paths.extend(failed_paths);
            }
        }

        // If there were any expansion errors, play warning sound and notify user with failed paths
        if any_expansion_error {
            println!("Warning: Issues encountered while expanding placeholders. Details are included in the prompt sent to Grok.");
            if !all_failed_paths.is_empty() {
                println!("Failed to expand placeholders for files/directories:");
                for path in &all_failed_paths {
                    println!("  {}", path);
                }
            }
            play_warning();
        }

        // Log the expanded messages (DEBUG level)
        log::debug!("Expanded messages for API request: {:?}", messages);

        // UPDATED: Conditionally build system content
        let mut system_content = String::new();
        if auto_request_files {
            system_content.push_str(get_system_instructions(provider));
        }
        if allow_file_writes {
            system_content.push_str(get_write_instructions(provider));
        }

        let mut api_messages = messages.clone();  // Clone to avoid mutating original
        
        // For Claude, system content goes in a separate field, not as a message
        if provider == "claude" {
            // Don't add system message to api_messages for Claude
        } else if !system_content.is_empty() {
            api_messages.insert(0, Message {
                role: "system".to_string(),
                content: system_content,
            });
        }

        // Get API key, build client
        let api_key = match provider {
            "claude" => env::var("ANTHROPIC_API_KEY").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "ANTHROPIC_API_KEY not set"))?,
            _ => env::var("XAI_API_KEY").map_err(|_| io::Error::new(io::ErrorKind::NotFound, "XAI_API_KEY not set"))?,
        };
        let client = Client::builder()
            .timeout(Duration::from_secs(api_timeout))
            .build()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // ------ NEW: Start timing here, before the inner loop ------
        let start_time = SystemTime::now();

        // Inner loop for handling truncation retries (in-memory, no file re-read)
        let mut needs_reprocess = false;
        loop {
            // Build the request
            let request_builder = if provider == "claude" {
                // Create Claude request
                let claude_req = ClaudeRequest {
                    model: model.to_string(),
                    messages: api_messages.clone(),
                    temperature: local_temperature,
                    max_tokens: parse_level(current_level),
                    system: if system_content.is_empty() { None } else { Some(system_content.clone()) },
                };

                // Log the full request (DEBUG level)
                log::debug!("Sending Claude API request: {:?}", claude_req);

                client
                    .post("https://api.anthropic.com/v1/messages")
                    .header("Content-Type", "application/json")
                    .header("x-api-key", &api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&claude_req)
            } else {
                // Create Grok request
                let grok_req = ChatRequest {
                    model: model.to_string(),
                    messages: api_messages.clone(),
                    temperature: local_temperature,
                    max_tokens: parse_level(current_level),
                };

                // Log the full request (DEBUG level)
                log::debug!("Sending Grok API request: {:?}", grok_req);

                client
                    .post("https://api.x.ai/v1/chat/completions")
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {}", api_key))
                    .json(&grok_req)
            };

            // Play thinking sound
            play_thinking();

            // Print thinking message with settings
            let ai_name = if provider == "claude" { "Claude" } else { "Grok" };
            println!("{} is thinking... (max_tokens: {}, temperature: {})", ai_name, parse_level(current_level), local_temperature);

            // Send and await
            let res = request_builder.send().await;

            match res {
                Ok(resp) if resp.status().is_success() => {
                    let (assistant_content, finish_reason) = if provider == "claude" {
                        let claude_resp: ClaudeResponse = resp.json().await.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                        let content = claude_resp.content.get(0).map(|c| c.text.clone()).unwrap_or_default();
                        let finish_reason = claude_resp.stop_reason.clone();
                        (content, finish_reason)
                    } else {
                        let chat_resp: ChatResponse = resp.json().await.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                        let content = chat_resp.choices[0].message.content.clone();
                        let finish_reason = chat_resp.choices[0].finish_reason.clone();
                        (content, finish_reason)
                    };

                    // UPDATED: Check if this is a file write request (now with path validation against allowed paths and flexible newline parsing)
                    let write_prefix = if provider == "claude" { "CLAUDE WRITES TO FILE:" } else { "GROK WRITES TO FILE:" };
                    let is_write_request = if allow_file_writes && !allowed_write_paths.is_empty() {
                        let trimmed = assistant_content.trim();
                        if trimmed.starts_with(write_prefix) {
                            // NEW: Robust parsing for single newline after header (find first \n after write prefix)
                            if let Some(nl_pos) = trimmed.find(write_prefix).map(|start| {
                                start + write_prefix.len()
                            }).and_then(|after_prefix| trimmed[after_prefix..].find('\n').map(|p| after_prefix + p)) {
                                let header_end = nl_pos + 1;  // After the \n
                                let header = trimmed[..header_end].trim();
                                let content = trimmed[header_end..].trim_start_matches(|c: char| c.is_whitespace() || c == '\n').to_string();  // Raw content, skipping extra whitespace/newlines

                                if content.is_empty() {
                                    log::warn!("Write response has empty content after header: {}", header);
                                    false
                                } else if header.starts_with(write_prefix) {
                                    let path_str = header.strip_prefix(write_prefix).unwrap().trim().to_string();

                                    // Validate against allowed paths from user's @w: placeholders
                                    if !allowed_write_paths.contains(&path_str) {
                                        log::warn!("Grok requested write to '{}', but it doesn't match any @w: path in user prompt ('{:?}'). Treating as normal response.", path_str, allowed_write_paths);
                                        false
                                    } else {
                                        log::debug!("Detected valid write request for path: {}", path_str);
                                        let path = PathBuf::from(&path_str);

                                        // Existing validation: relative, no traversal, within cwd
                                        let cwd = env::current_dir()?;
                                        if path.is_absolute() || path_str.starts_with("..") || path_str.contains("..") || contains_traversal(&path_str) {
                                            println!("Warning: Invalid path for write (traversal or absolute): {}", path_str);
                                            false
                                        } else {
                                            // Resolve full path relative to cwd
                                            let full_path = cwd.join(&path);
                                            if let Some(parent) = full_path.parent() {
                                                if let Err(e) = fs::create_dir_all(parent) {
                                                    println!("Warning: Failed to create parent directories for {}: {}", path_str, e);
                                                    // Still try to write, but may fail below
                                                }
                                            }

                                            match File::create(&full_path) {  // This overwrites the entire file (truncates to 0 length)
                                                Ok(mut file) => {
                                                    if let Err(e) = file.write_all(content.as_bytes()) {
                                                        println!("Warning: Failed to write to {}: {}", path_str, e);
                                                        false
                                                    } else {
                                                        println!("Successfully overwrote '{}' with generated content ({} bytes).", full_path.display(), content.len());
                                                        // Append confirmation to chat file (not the full content to save tokens/space)
                                                        if let Err(e) = fs::OpenOptions::new()
                                                            .append(true)
                                                            .open(chat_path)
                                                            .and_then(|mut f| {
                                                                writeln!(f, "\n{}:\nGenerated and overwrote content in {}.\n(Full content saved to the file; check it there.)\n\n{}:\n",
                                                                    GROK_RESPONSE_MARKER, path.display(), USER_PROMPT_MARKER)?;
                                                                Ok(())
                                                            })
                                                        {
                                                            println!("Warning: Failed to append confirmation to chat: {}", e);
                                                        }
                                                        play_chime();
                                                        true  // Handled successfully
                                                    }
                                                }
                                                Err(e) => {
                                                    println!("Warning: Failed to create/overwrite file {}: {}", path_str, e);
                                                    false
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    false
                                }
                            } else {
                                log::warn!("Write response missing newline after 'GROK WRITES TO FILE:': {}", trimmed);
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // If it was a write request, break inner loop (already handled above)
                    if is_write_request {
                        needs_reprocess = false;  // No reprocess needed for writes
                        break;
                    }

                    // Check if this is a file request (only if flag is enabled)
                    let file_request_prefix = if provider == "claude" { "CLAUDE REQUESTS FILES:" } else { "GROK REQUESTS FILES:" };
                    let is_file_request = if auto_request_files {
                        let trimmed = assistant_content.trim();
                        if trimmed.starts_with(file_request_prefix) {
                            let rest = trimmed.strip_prefix(file_request_prefix).unwrap().trim();
                            // Ensure it's exactly the format (no extra content)
                            if !rest.is_empty() && trimmed == format!("{} {}", file_request_prefix, rest) {
                                let paths: Vec<String> = rest.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

                                // Validate paths (syntactic only: no absolute, no traversal)
                                let mut all_valid = true;
                                let mut valid_paths = vec![];
                                for p in paths.iter() {
                                    let path = PathBuf::from(p);
                                    if path.is_absolute() || p.starts_with("..") || p.contains("..") || contains_traversal(p) {
                                        println!("Warning: Invalid path requested (traversal or absolute): {}", p);
                                        all_valid = false;
                                        break;
                                    }
                                    valid_paths.push(p.clone());
                                }

                                if all_valid && !valid_paths.is_empty() {
                                    // Append visible note and placeholders to the END of the file (augments the last USER PROMPT)
                                    let mut file = fs::OpenOptions::new().append(true).open(chat_path)?;
                                    let ai_name = if provider == "claude" { "CLAUDE" } else { "GROK" };
                                    writeln!(file, "\n\n{} REQUESTED FILES:", ai_name)?;
                                    for vp in valid_paths {
                                        writeln!(file, "@f:{}", vp)?;  // No space after 'f'
                                    }

                                    // Set flag to reprocess (re-read file) and break inner loop
                                    needs_reprocess = true;
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // Check if this is an RG request (only if flag is enabled) -- Added
                    let rg_prefix = if provider == "claude" { "CLAUDE RUNS RG:" } else { "GROK RUNS RG:" };
                    let is_rg_request = if allow_rg_commands {
                        let trimmed = assistant_content.trim();
                        if trimmed.starts_with(rg_prefix) {
                            let rest = trimmed.strip_prefix(rg_prefix).unwrap().trim();
                            if !rest.is_empty() && trimmed == format!("{} {}", rg_prefix, rest) {
                                let cwd = env::current_dir()?;
                                match run_rg_command(rest, &cwd).await {
                                    Ok(output) => {
                                        // Append to file
                                        let mut file = fs::OpenOptions::new().append(true).open(chat_path)?;
                                        let ai_name = if provider == "claude" { "CLAUDE" } else { "GROK" };
                                        writeln!(file, "\n\n{} RAN RG: {}\n```\n{}\n```\n", ai_name, rest, output)?;
                                        needs_reprocess = true;  // Set needs_reprocess
                                        true  // Set is_rg_request
                                    }
                                    Err(e) => {
                                        log::warn!("Failed RG command '{}': {}", rest, e);
                                        play_warning();  // Optional
                                        false
                                    }
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // Check if this is an FD request (only if flag is enabled) -- Added
                    let fd_prefix = if provider == "claude" { "CLAUDE RUNS FD:" } else { "GROK RUNS FD:" };
                    let is_fd_request = if allow_fd_commands {
                        let trimmed = assistant_content.trim();
                        if trimmed.starts_with(fd_prefix) {
                            let rest = trimmed.strip_prefix(fd_prefix).unwrap().trim();
                            if !rest.is_empty() && trimmed == format!("{} {}", fd_prefix, rest) {
                                let cwd = env::current_dir()?;
                                match run_fd_command(rest, &cwd).await {
                                    Ok(output) => {
                                        // Append to file
                                        let mut file = fs::OpenOptions::new().append(true).open(chat_path)?;
                                        let ai_name = if provider == "claude" { "CLAUDE" } else { "GROK" };
                                        writeln!(file, "\n\n{} RAN FD: {}\n```\n{}\n```\n", ai_name, rest, output)?;
                                        needs_reprocess = true;  // Set is_fd_request
                                        true  // Set is_fd_request
                                    }
                                    Err(e) => {
                                        log::warn!("Failed FD command '{}': {}", rest, e);
                                        play_warning();  // Optional
                                        false
                                    }
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    // If it was a valid file request, break inner loop to allow outer loop to re-read
                    if is_file_request {
                        break;
                    }

                    // If it was a valid RG request, break inner loop to re-read -- Added
                    if is_rg_request {
                        break;
                    }

                    // If it was a valid FD request, break inner loop to re-read -- Added
                    if is_fd_request {
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
                    // ------ NEW: Calculate elapsed time ------
                    let elapsed = start_time.elapsed().map(|d| d.as_secs()).unwrap_or(0);
                    let ai_name = if provider == "claude" { "Claude" } else { "Grok" };
                    println!("{} has thought ({} seconds).", ai_name, elapsed);

                    let mut file = fs::OpenOptions::new().append(true).open(chat_path)?;
                    let response_marker = if provider == "claude" { "CLAUDE RESPONSE" } else { GROK_RESPONSE_MARKER };
                    writeln!(
                        file,
                        "\n{}:\n{}\n\n{}:\n",
                        response_marker,
                        assistant_content,
                        USER_PROMPT_MARKER
                    )?;

                    // If still truncated at max level, print warning
                    if is_truncated {
                        println!("Warning: Response truncated even at max L{} ({} tokens)!", MAX_LEVEL, parse_level(MAX_LEVEL));
                    }

                    // Play chime sound
                    play_chime();

                    // Break inner loop after handling final response
                    break;
                }
                Ok(resp) => {
                    let status = resp.status();
                    let err_body = resp.text().await.unwrap_or_default();
                    let ai_name = if provider == "claude" { "Claude" } else { "Grok" };
                    println!("{} failed to respond.", ai_name);
                    play_warning();
                    return Err(io::Error::new(io::ErrorKind::Other, format!("API error: {} - Body: {}", status, err_body)));
                }
                Err(e) => {
                    let ai_name = if provider == "claude" { "Claude" } else { "Grok" };
                    println!("{} failed to respond.", ai_name);
                    play_warning();
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
        if line.trim() == "USER PROMPT:" || line.trim() == "GROK RESPONSE:" || line.trim() == "CLAUDE RESPONSE:" {
            // Add previous section if content is non-empty
            let trimmed = current_content.trim().to_string();
            if !trimmed.is_empty() {
                let role = current_role.take().unwrap_or_else(|| "user".to_string());
                messages.push(Message {
                    role,
                    content: trimmed,
                });
            }

            // Start new section
            current_role = Some(if line.trim() == "USER PROMPT:" { "user".to_string() } else { "assistant".to_string() });
            current_content.clear();
        } else {
            // Append to current content
            writeln!(&mut current_content, "{}", line).expect("Failed to write to String");
        }
    }

    // Add the last section if content is non-empty
    let trimmed = current_content.trim().to_string();
    if !trimmed.is_empty() {
        let role = current_role.unwrap_or_else(|| "user".to_string());
        messages.push(Message {
            role,
            content: trimmed,
        });
    }

    messages
}

fn expand_placeholders(text: &str) -> io::Result<(String, bool, Vec<String>)> {
    let re = Regex::new(r#"@f\s*:\s*([^\s<>"'`]+)|@d\s*:\s*([^\s<>"'`]+)"#).unwrap();
    let mut result = String::new();
    let mut last_end = 0;
    let mut had_error = false;
    let mut failed_paths = Vec::new(); // New: Collect failed paths here

    let cwd = env::current_dir()?;

    for cap in re.captures_iter(text) {
        let match_range = cap.get(0).unwrap();
        let match_start = match_range.start();
        result.push_str(&text[last_end..match_start]);

        if let Some(file_path) = cap.get(1) {
            let path_str = file_path.as_str();
            let (expanded, err, failed_for_this) = expand_file_path(path_str, &cwd); // Updated signature
            result.push_str(&expanded);
            had_error |= err;
            failed_paths.extend(failed_for_this); // Add to overall list
        } else if let Some(dir_path) = cap.get(2) {
            let path_str = dir_path.as_str();
            let (expanded, err, failed_for_this) = expand_dir_tree(path_str, &cwd); // Updated signature
            result.push_str(&expanded);
            had_error |= err;
            failed_paths.extend(failed_for_this); // Add to overall list
        }

        last_end = match_range.end();
    }

    result.push_str(&text[last_end..]);
    Ok((result, had_error, failed_paths))
}

fn expand_file_path(path_str: &str, cwd: &Path) -> (String, bool, Vec<String>) {
    let path = Path::new(path_str);
    let mut output = String::new();
    let mut had_error = false;
    let mut failed_paths = Vec::new(); // New: Collect failed paths for this expansion

    if path_str.contains('*') || path_str.contains('?') {
        // Glob
        match glob(path_str) {
            Ok(iter) => {
                let mut files: Vec<_> = iter.filter_map(|res| res.ok()).filter(|p| p.is_file()).collect();
                if files.is_empty() {
                    had_error = true;
                    failed_paths.push(path_str.to_string()); // Add failed glob pattern
                    let _ = writeln!(output, "No files matched the pattern {}.\n", path_str);
                } else {
                    files.sort();
                    for p in files {
                        match p.canonicalize() {
                            Ok(canon) if canon.starts_with(cwd) => {
                                match fs::read_to_string(&p) {
                                    Ok(content) => {
                                        let _ = writeln!(output, "Contents of {}:\n```\n{}\n```\n", p.display(), content);
                                    }
                                    Err(e) => {
                                        had_error = true;
                                        failed_paths.push(p.display().to_string()); // Add failed file
                                        let _ = writeln!(output, "Failed to read file {}: {}.\n", p.display(), e);
                                    }
                                }
                            }
                            _ => {
                                had_error = true;
                                failed_paths.push(p.display().to_string()); // Add invalid/outside file
                                let _ = writeln!(output, "The requested file {} is unavailable (outside project or invalid).\n", p.display());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                had_error = true;
                failed_paths.push(path_str.to_string()); // Add invalid glob pattern
                let _ = writeln!(output, "Invalid glob pattern {}: {}.\n", path_str, e);
            }
        }
    } else if path.is_dir() {
        // Directory recurse
        if !path.exists() {
            had_error = true;
            failed_paths.push(path_str.to_string()); // Add non-existent directory
            let _ = writeln!(output, "The requested directory {} does not exist.\n", path_str);
        } else if !path.is_dir() {
            had_error = true;
            failed_paths.push(path_str.to_string()); // Add invalid (not a dir)
            let _ = writeln!(output, "The path {} is not a directory.\n", path_str);
        } else {
            match path.canonicalize() {
                Ok(canon) if canon.starts_with(cwd) => {
                    let mut entries: Vec<_> = WalkDir::new(path).into_iter().filter_map(|e| e.ok()).filter(|e| e.file_type().is_file()).collect();
                    if entries.is_empty() {
                        let _ = writeln!(output, "No files found in directory {}.\n", path.display());
                        // Note: Not considering empty dir as error
                    } else {
                        entries.sort_by_key(|e| e.path().to_owned());
                        for entry in entries {
                            let ep = entry.path();
                            match ep.canonicalize() {
                                Ok(canon) if canon.starts_with(cwd) => {
                                    match fs::read_to_string(ep) {
                                        Ok(content) => {
                                            let _ = writeln!(output, "Contents of {}:\n```\n{}\n```\n", ep.display(), content);
                                        }
                                        Err(e) => {
                                            had_error = true;
                                            failed_paths.push(ep.display().to_string()); // Add failed file in dir
                                            let _ = writeln!(output, "Failed to read file {}: {}.\n", ep.display(), e);
                                        }
                                    }
                                }
                                _ => {
                                    had_error = true;
                                    failed_paths.push(ep.display().to_string()); // Add invalid file in dir
                                    let _ = writeln!(output, "The requested file {} is unavailable.\n", ep.display());
                                }
                            }
                        }
                    }
                }
                _ => {
                    had_error = true;
                    failed_paths.push(path_str.to_string()); // Add invalid/outside directory
                    let _ = writeln!(output, "The requested directory {} is unavailable (outside project or invalid).\n", path_str);
                }
            }
        }
    } else {
        // Single file
        match path.canonicalize() {
            Ok(canon) if canon.starts_with(cwd) => {
                match fs::read_to_string(path) {
                    Ok(content) => {
                        let _ = writeln!(output, "Contents of {}:\n```\n{}\n```\n", path.display(), content);
                    }
                    Err(e) => {
                        had_error = true;
                        failed_paths.push(path_str.to_string()); // Add failed single file
                        let _ = writeln!(output, "Failed to read file {}: {}.\n", path.display(), e);
                    }
                }
            }
            _ => {
                // Covers not found, outside project, etc.
                had_error = true;
                failed_paths.push(path_str.to_string()); // Add missing/invalid single file
                let _ = writeln!(output, "The requested file {} does not exist or is unavailable.\n", path_str);
            }
        }
    }

    (output, had_error, failed_paths)
}

fn expand_dir_tree(path_str: &str, cwd: &Path) -> (String, bool, Vec<String>) {
    let path = Path::new(path_str);
    let mut had_error = false;
    let mut failed_paths = Vec::new(); // New: Collect failed paths for this expansion

    if !path.exists() {
        had_error = true;
        failed_paths.push(path_str.to_string()); // Add non-existent directory
        return (format!("The requested directory {} does not exist.\n", path_str), had_error, failed_paths);
    }
    if !path.is_dir() {
        had_error = true;
        failed_paths.push(path_str.to_string()); // Add invalid (not a dir)
        return (format!("The path {} is not a directory.\n", path_str), had_error, failed_paths);
    }

    match path.canonicalize() {
        Ok(canon) if canon.starts_with(cwd) => {
            let mut output = format!("Contents of directory {}:\n```\n", path.display());
            let mut entries: Vec<_> = WalkDir::new(path).min_depth(1).into_iter().filter_map(|e| e.ok()).collect();
            if entries.is_empty() {
                output.push_str("(empty directory)\n");
                // Note: Not considering empty dir as error
            } else {
                entries.sort_by_key(|e| e.path().to_owned());
                for entry in entries {
                    let rel_path = entry.path().strip_prefix(path).unwrap();
                    let indent = "  ".repeat(entry.depth() - 1);
                    if entry.file_type().is_dir() {
                        let _ = writeln!(output, "{}{}/", indent, rel_path.display());
                    } else {
                        let _ = writeln!(output, "{}{}", indent, rel_path.display());
                    }
                }
            }
            output.push_str("```\n");
            (output, had_error, failed_paths) // No failures for tree expansion itself, only initial checks
        }
        _ => {
            had_error = true;
            failed_paths.push(path_str.to_string()); // Add invalid/outside directory
            (format!("The requested directory {} is unavailable (outside project or invalid).\n", path_str), had_error, failed_paths)
        }
    }
}

// Added: Safety check for RG commands
fn is_safe_rg_command(command_line: &str) -> bool {
    // Basic safety check: must start with "rg "
    if !command_line.trim().starts_with("rg ") {
        return false;
    }
    // Parse args safely
    let args = match split(command_line.trim()) {
        Ok(a) => a,
        Err(_) => return false,  // Invalid shell-like syntax
    };
    if args.first() != Some(&"rg".to_string()) {
        return false;
    }
    // Whitelist safe flags (example: add more as needed)
    let safe_flags = vec!["--line-number", "-n", "--case-insensitive", "-i", "--fixed-strings", "-F", "--word-regexp", "-w", "--after-context", "-A", "--before-context", "-B", "--context", "-C", "--type", "--type-add", "--max-columns", "--glob"];
    for arg in &args[1..] {
        if arg.contains(&['|', '>', '<', '&', ';'][..]) || arg.starts_with("../") || Path::new(arg).is_absolute() {
            return false;  // Forbidden metachar or absolute/traversal
        }
        // Allow numeric args after flags like -A
        if safe_flags.contains(&arg.as_str()) {
            continue;
        }
        // Allow numbers after certain flags (simple check)
        if arg.chars().all(|c| c.is_numeric()) {
            continue;  // e.g., 5 after -A
        }
        // Allow patterns (strings) and paths if not starting with -
        if !arg.starts_with('-') {
            continue;  // Assume patterns/paths are okay if not flag-like
        }
        return false;  // Unknown/unallowed flag
    }
    true
}

// Added: Execute safe RG command
async fn run_rg_command(command_line: &str, cwd: &Path) -> io::Result<String> {
    if !is_safe_rg_command(command_line) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid or unsafe RG command"));
    }
    let args = split(command_line.trim()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("Parse error: {}", e)))?;
    let mut cmd = std::process::Command::new("rg");
    cmd.args(&args[1..]).current_dir(cwd).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    // Timeout using tokio::time
    match tokio::time::timeout(Duration::from_secs(RG_TIMEOUT_SECS), tokio::process::Command::from(cmd).output()).await {
        Ok(Ok(output)) => {
            let mut result = String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);
            if result.len() > MAX_RG_OUTPUT_BYTES {
                result.truncate(MAX_RG_OUTPUT_BYTES);
                result.push_str("\n[Output truncated]\n");
            }
            Ok(result)
        }
        Ok(Err(e)) => Ok(format!("RG command failed: {}\n", e)),
        Err(_) => Ok("RG command timed out.\n".to_string()),
    }
}

// Added: Safety check for FD commands
fn is_safe_fd_command(command_line: &str) -> bool {
    // Basic safety check: must start with "fd "
    if !command_line.trim().starts_with("fd ") {
        return false;
    }
    // Parse args safely
    let args = match split(command_line.trim()) {
        Ok(a) => a,
        Err(_) => return false,  // Invalid shell-like syntax
    };
    if args.first() != Some(&"fd".to_string()) {
        return false;
    }
    // Whitelist safe flags (add more as needed based on fd docs)
    let safe_flags = vec!["--type", "--glob", "--max-depth", "--min-depth", "--size", "--changed-before", "--changed-within", "--exclude"];
    for arg in &args[1..] {
        if arg.contains(&['|', '>', '<', '&', ';'][..]) || arg.starts_with("../") || Path::new(arg).is_absolute() {
            return false;  // Forbidden metachar or absolute/traversal
        }
        // Allow flags in whitelist
        if safe_flags.contains(&arg.as_str()) {
            continue;
        }
        // Allow strings/patterns if not starting with -
        if !arg.starts_with('-') {
            continue;  // Assume paths/patterns are okay
        }
        return false;  // Unknown/unallowed flag
    }
    true
}

// Added: Execute safe FD command
async fn run_fd_command(command_line: &str, cwd: &Path) -> io::Result<String> {
    if !is_safe_fd_command(command_line) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid or unsafe FD command"));
    }
    let args = split(command_line.trim()).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("Parse error: {}", e)))?;
    let mut cmd = std::process::Command::new("fd");
    cmd.args(&args[1..]).current_dir(cwd).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    // Timeout
    match tokio::time::timeout(Duration::from_secs(FD_TIMEOUT_SECS), tokio::process::Command::from(cmd).output()).await {
        Ok(Ok(output)) => {
            let mut result = String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr);
            if result.len() > MAX_FD_OUTPUT_BYTES {
                result.truncate(MAX_FD_OUTPUT_BYTES);
                result.push_str("\n[Output truncated]\n");
            }
            Ok(result)
        }
        Ok(Err(e)) => Ok(format!("FD command failed: {}\n", e)),
        Err(_) => Ok("FD command timed out.\n".to_string()),
    }
}

// Play thinking sound (non-blocking, errors logged)
fn play_thinking() {
    tokio::spawn(async {
        if let Err(e) = tokio::task::spawn_blocking(|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let (_stream, stream_handle) = OutputStream::try_default().map_err(|e| format!("Failed to get default output stream: {}", e))?;
            let sink = Sink::try_new(&stream_handle).map_err(|e| format!("Failed to create sink: {}", e))?;
            
            let bytes = include_bytes!("../media/thinking.mp3");
            let cursor = Cursor::new(bytes.as_ref());
            let source = Decoder::new(cursor).map_err(|e| format!("Failed to decode MP3: {}", e))?;
            
            sink.append(source);
            sink.sleep_until_end();
            Ok(())
        }).await {
            log::error!("Failed to spawn or complete thinking sound playback: {}", e);
        }
    });
}

// Play chime sound (non-blocking, errors logged)
fn play_chime() {
    tokio::spawn(async {
        if let Err(e) = tokio::task::spawn_blocking(|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let (_stream, stream_handle) = OutputStream::try_default().map_err(|e| format!("Failed to get default output stream: {}", e))?;
            let sink = Sink::try_new(&stream_handle).map_err(|e| format!("Failed to create sink: {}", e))?;
            
            let bytes = include_bytes!("../media/chime.mp3");
            let cursor = Cursor::new(bytes.as_ref());
            let source = Decoder::new(cursor).map_err(|e| format!("Failed to decode MP3: {}", e))?;
            
            sink.append(source);
            sink.sleep_until_end();
            Ok(())
        }).await {
            log::error!("Failed to spawn or complete chime sound playback: {}", e);
        }
    });
}

// Play warning sound (non-blocking, errors logged)
fn play_warning() {
    tokio::spawn(async {
        if let Err(e) = tokio::task::spawn_blocking(|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let (_stream, stream_handle) = OutputStream::try_default().map_err(|e| format!("Failed to get default output stream: {}", e))?;
            let sink = Sink::try_new(&stream_handle).map_err(|e| format!("Failed to create sink: {}", e))?;
            
            let frequencies = [659, 523, 440];
            for freq in frequencies {
                let source = SineWave::new(freq as f32).take_duration(StdDuration::from_millis(200)).amplify(0.20);
                sink.append(source);
                std::thread::sleep(StdDuration::from_millis(50));
            }
            sink.sleep_until_end();
            Ok(())
        }).await {
            log::error!("Failed to spawn or complete warning sound playback: {}", e);
        }
    });
}
