# Grok Chat File Watcher

This works for me. I run it in the root of whatever project I'm working on and then edit and read the generated chat file. I work in `nvim`.

I edit the chat file as I go, often just deleting everything but the opening prompt to save on processing time.

An example of an opening prompt could be:

```md
You are a Rust expert.
You are assisting me in developing a Grok 4 chat utility.
All the code for the project is here: @f./src/main.rs
There is a media file: @d:./media
The crates are detailed here: @f:.Cargo.toml

For starters, please could you make the help message a bit more friendly?
```

The utility sees the change to the chat file, sends it to Grok 4's API, and then outputs the response to the same file.

Like I say, it's working for me. :)

---

A Rust utility that enables interactive conversations with the Grok API (from xAI) by monitoring a Markdown chat file. The app polls the file every 1 second for changes. When it detects a new user prompt (marked by "USER PROMPT:"), it sends the full conversation history to the Grok API, appends the response (marked by "GROK RESPONSE:"), and adds a new "USER PROMPT:" section for your next input. It plays a pleasant chime sound on successful responses and a warning sound on errors.

This tool is ideal for users who prefer editing a file in their favorite text editor (e.g., VS Code, Vim) rather than using a web interface or CLI prompt. It supports placeholders for including file contents, directory listings, and per-prompt token limits.

## Features
- **File Watching**: Polls the chat file (default: `./gchat.md`) every 1 second. Processes changes automatically.
- **Conversation History**: Builds and sends the full history as a list of user/assistant messages.
- **Placeholders in Prompts**:
  - `@f :path`: Includes the contents of a file, glob pattern (e.g., `./*.rs`), or entire directory (recursively).
  - `@d :path`: Includes a tree listing of a directory's contents (files and subdirs).
  - `@t :L<level>`: Sets the `max_tokens` for that specific prompt (e.g., `@t :L3` for 4096 tokens). Overrides the default; the last one in the prompt wins.
- **Audio Feedback**: Chime on success, warning tones on failure (requires audio dependencies for `rodio`).
- **Logging**: Configure via `RUST_LOG` environment variable (e.g., `RUST_LOG=debug` for detailed output, including API requests/responses).
- **Truncation Handling**: Warns if the API response is truncated due to token limits.
- **Initial Processing**: On startup, processes any pending user prompt in the file.

## Installation

### Prerequisites
- **Rust**: Install from [rustup.rs](https://rustup.rs/). Requires Rust 1.70+.
- **Audio Dependencies** (for sounds): On Linux, install `libasound2-dev` and `pkg-config` (e.g., `sudo apt install libasound2-dev pkg-config`). On macOS/Windows, it should work out-of-the-box with `rodio`.
- **Grok API Key**: Sign up at [x.ai](https://x.ai) and get your API key.

### Building the Project
1. Clone the repository:
   ```
   git clone <repository-url>
   cd <repository-dir>
   ```

2. Build and run with Cargo:
   ```
   cargo build --release
   ```
   The executable will be in `target/release/<binary-name>` (binary name is based on your `Cargo.toml`, e.g., `grok-chat-watcher`).

   To run directly:
   ```
   cargo run --release -- [options]
   ```

### Dependencies
The project uses:
- `clap` for command-line parsing.
- `reqwest` and `tokio` for async API calls.
- `serde` for JSON handling.
- `regex` and `walkdir`/`glob` for placeholder expansion.
- `rodio` for audio feedback.
- `log` and `env_logger` for logging.

All are pulled in via `Cargo.toml` during build.

## Setup
1. **Set API Key**:
   Export your Grok API key as an environment variable:
   ```
   export XAI_API_KEY=your-api-key-here
   ```
   (Add this to your shell profile, e.g., `~/.bashrc`, for persistence.)

2. **Optional: Enable Logging**:
   Set `RUST_LOG` for output levels:
   - `RUST_LOG=info` (default, basic info).
   - `RUST_LOG=debug` (detailed, including full API requests/responses).
   Example:
   ```
   export RUST_LOG=debug
   ```

3. **Run the App**:
   ```
   cargo run -- [options]
   ```
   The app runs indefinitely until stopped (e.g., Ctrl+C). It creates the chat file if it doesn't exist.

## Usage

### Command-Line Options
Use `cargo run -- --help` for full details. Key options:
- `-f, --chat-file <PATH>`: Path to the chat file (default: `./gchat.md`).
- `-t, --max-tokens <LEVEL>`: Default max tokens level (e.g., `L5` for 16384 tokens). Can be overridden per-prompt with `@t`. See "Token Levels" below for details.
- `-T, --api-timeout <SECONDS>`: API request timeout (default: 600 seconds).

Example:
```
cargo run -- -f mychat.md -t L3 -T 300
```

### Basic Workflow
1. Start the app. It will create `./gchat.md` (or your specified file) if needed, with an initial "USER PROMPT:" marker.
2. Open the chat file in your text editor and add your prompt under "USER PROMPT:".
3. Save the file. The app detects the change, sends the conversation to Grok, appends the response, and adds a new "USER PROMPT:" section.
4. Repeat: Edit the new "USER PROMPT:" section, save, and wait for the response.

Example chat file content (`gchat.md`):
```
USER PROMPT:
Hello, Grok!

GROK RESPONSE:
Hello! How can I help you today?

USER PROMPT:
What's the meaning of life? @t :L2  (This will be processed next, with max_tokens=2048)
```

- The app only processes if the last section is a non-empty "USER PROMPT:".
- On startup, it processes any pending prompt immediately.
- While processing, it prints "Grok is thinking...". On completion: "Grok has thought." and plays a chime.
- Errors print details and play a warning sound.

### Placeholders in User Prompts
Placeholders are expanded **only in "USER PROMPT:" sections** before sending to the API. They are removed/replaced in the sent prompt.

- **File Contents (`@f :path`)**:
  - Single file: `@f :./example.txt` → Inserts "Contents of ./example.txt:\n```\n[file content]\n```\n".
  - Glob: `@f :./src/*.rs` → Inserts contents of all matching files, sorted.
  - Directory: `@f :./src` → Recursively inserts contents of all files in the directory, sorted.
  - Errors (e.g., file not found) print warnings and leave the placeholder unexpanded.

- **Directory Tree (`@d :path`)**:
  - `@d :./src` → Inserts a tree listing like "Contents of directory ./src:\n```\nsrc/main.rs\nsrc/utils/\nsrc/utils/helper.rs\n```\n".
  - Recurses through subdirectories; errors print warnings.

- **Max Tokens (`@t :L<level>`)**:
  - Sets `max_tokens` for that prompt only (overrides CLI default).
  - Example: `@t :L4` → 8192 tokens.
  - Multiple in one prompt: Last one wins.
  - Removed after processing.
  - See "Token Levels" below.

Placeholders are case-sensitive and must be formatted exactly (e.g., space after `@f`, colon before path).

### Token Levels (L* Parameters)
The `--max-tokens` option and `@t` placeholder use "L" levels to specify `max_tokens` (the maximum tokens in the API response). Levels are powers of 2 starting from 512:

- **L0**: 512 tokens
- **L1**: 1024 tokens
- **L2**: 2048 tokens
- **L3**: 4096 tokens
- **L4**: 8192 tokens
- **L5**: 16384 tokens (maximum; higher levels are capped at L5)

Format: `L<digit>`, e.g., `L5`. Invalid formats (e.g., `L6` or `512`) will error or cap at L5 with a warning.

- CLI: `--max-tokens L3` sets default to 4096.
- Per-prompt: `@t :L2` in the user prompt overrides to 2048 for that request.
- If the response is truncated (hits the limit), a warning is printed: "Warning: Response truncated due to max_tokens limit!"

Higher levels allow longer responses but may increase API costs/latency.

## Notes
- **Polling**: Checks every 1 second; includes a 500ms debounce after detection to handle file saves.
- **File Format**: Must use exact markers ("USER PROMPT:" and "GROK RESPONSE:") on their own lines. Content follows until the next marker.
- **API Model**: Hardcoded to "grok-4-0709" with temperature=1.0.
- **Errors**: API failures (e.g., invalid key, timeouts) print to console and play a warning sound. Check logs for details.
- **Sounds**: Bundled MP3 chime for success; generated descending tones for warnings. Disable by removing `rodio` calls if desired.
- **Limitations**: No multi-user support; single-threaded polling. API rate limits/costs apply (check xAI docs).
- **Contributing**: Open issues/PRs on the repository.

For questions, see the in-app help (`--help`) or source code. Enjoy chatting with Grok! 🚀
