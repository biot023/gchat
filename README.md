# Simple Grok 4 Dev 'Assistant'

This works for me. I run it in the root of whatever project I'm working on and then edit and read the generated chat file. I work in `nvim`.

I edit the chat file as I go, often just deleting everything but the opening prompt to save on processing time.

An example of an opening prompt could be:

```md
You are a Rust expert.
You are assisting me in developing a Grok 4 chat utility.
All the code for the project is here: @f:./src
There is a media file: @d:./media
The crates are detailed here: @f:./Cargo.toml

For starters, please could you make the help message a bit more friendly?
```

The utility sees the change to the chat file, sends it to Grok 4's API, and then outputs the response to the same file.

Like I say, it's working for me. :)

---

A Rust utility that enables interactive conversations with the Grok API (from xAI) by monitoring a Markdown chat file. The app polls the file every 1 second for changes. When it detects a new user prompt (marked by "USER PROMPT:"), it sends the full conversation history to the Grok API, appends the response (marked by "GROK RESPONSE:"), and adds a new "USER PROMPT:" section for your next input. It plays a pleasant chime sound on successful responses and a warning sound on errors.

This tool is ideal for users who prefer editing a file in their favorite text editor (e.g., VS Code, Vim) rather than using a web interface or CLI prompt. It supports placeholders for including file contents, directory listings, per-prompt token limits, and temperature settings.

## Features
- **File Watching**: Polls the chat file (default: `./gchat.md`) every 1 second. Processes changes automatically.
- **Conversation History**: Builds and sends the full history as a list of user/assistant messages.
- **Placeholders in Prompts**:
  - `@f:path`: Includes the contents of a file, glob pattern (e.g., `./*.rs`), or entire directory (recursively). Note: No space after `@f` in the placeholder (e.g., `@f:./src/main.rs`), though the app can handle optional spaces.
  - `@d:path`: Includes a tree listing of a directory's contents (files and subdirs).
  - `@t:L<level>`: Sets the `max_tokens` for that specific prompt (e.g., `@t:L3` for 4096 tokens). Overrides the default; the last one across all user messages in history wins.
  - `@p:<value>`: Sets the `temperature` for that specific prompt (e.g., `@p:0.9`). Overrides the default; the last one across all user messages in history wins. Value is a float (e.g., 0.0 to 2.0).
- **Audio Feedback**: Chime on success, warning tones on failure (requires audio dependencies for `rodio`).
- **Logging**: Configure via `RUST_LOG` environment variable (e.g., `RUST_LOG=debug` for detailed output, including API requests/responses).
- **Truncation Handling**: Warns if the API response is truncated due to token limits. Optional auto-increase feature to retry with higher limits.
- **Initial Processing**: On startup, processes any pending user prompt in the file.
- **Auto File Requests**: Optional feature (enabled with `--auto-request-files` or `-a`). Allows Grok to request files from your project directory if needed to answer queries. Grok responds in a specific format ("GROK REQUESTS FILES: relative/path1, relative/path2"), and the utility automatically appends placeholders (e.g., `@f:src/main.rs`) to the last user prompt, then re-queries the API with the contents included. This chains until a normal response is received. Paths are validated to stay within the project directory (no absolute paths or parent traversal). Supports globs and directories if requested.
- **Auto-Increase Max Tokens**: Optional feature (enabled with `--auto-increase-max-tokens` or `-i`). Automatically retries truncated responses with incrementally higher `max_tokens` levels (up to L5) until non-truncated or max is reached.

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
   The executable will be in `target/release/gchat` (binary name based on `Cargo.toml`).

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
- `-t, --max-tokens <LEVEL>`: Default max tokens level (default: `L3` for 4096 tokens). Can be overridden per-prompt with `@t`. See "Token Levels" below for details.
- `-p, --temperature <FLOAT>`: Default temperature (default: 1.0). Can be overridden per-prompt with `@p`.
- `-m, --model <STRING>`: The Grok model to call (default: `grok-4`).
- `-T, --api-timeout <SECONDS>`: API request timeout (default: 600 seconds).
- `-a, --auto-request-files`: Enable Grok to automatically request and include project files if needed (default: false). See "Auto File Requests" below for details.
- `-i, --auto-increase-max-tokens`: Automatically increase max_tokens level on truncation (up to L5) by re-querying (default: false). See "Auto-Increase Max Tokens" below for details.

Example:
```
cargo run -- -f mychat.md -t L3 -p 0.8 -m grok-4 -T 300 -a -i
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
What's the meaning of life? @t:L2 @p:0.5  (This will be processed next, with max_tokens=2048 and temperature=0.5)
```

- The app only processes if the last section is a non-empty "USER PROMPT:".
- On startup, it processes any pending prompt immediately.
- While processing, it prints "Grok is thinking...". On completion: "Grok has thought." and plays a chime.
- Errors print details and play a warning sound.

### Placeholders in User Prompts
Placeholders are expanded **only in "USER PROMPT:" sections** before sending to the API. They are removed/replaced in the sent prompt.

- **File Contents (`@f:path`)**:
  - Single file: `@f:./example.txt` → Inserts "Contents of ./example.txt:\n```\n[file content]\n```\n".
  - Glob: `@f:./src/*.rs` → Inserts contents of all matching files, sorted.
  - Directory: `@f:./src` → Recursively inserts contents of all files in the directory, sorted.
  - Errors (e.g., file not found) print warnings and leave the placeholder unexpanded.

- **Directory Tree (`@d:path`)**:
  - `@d:./src` → Inserts a tree listing like "Contents of directory ./src:\n```\nsrc/main.rs\nsrc/utils/\nsrc/utils/helper.rs\n```\n".
  - Recurses through subdirectories; errors print warnings.

- **Max Tokens (`@t:L<level>`)**:
  - Sets `max_tokens` for that prompt (overrides CLI default).
  - Example: `@t:L4` → 8192 tokens.
  - Last one across all user messages wins; removed after processing.
  - See "Token Levels" below.

- **Temperature (`@p:<value>`)**:
  - Sets `temperature` for that prompt (overrides CLI default).
  - Example: `@p:1.2` → temperature=1.2.
  - Last one across all user messages wins; removed after processing.
  - Typical range: 0.0 (deterministic) to 2.0 (more creative).

Placeholders are case-sensitive and must be formatted exactly (e.g., no space after `@f`, colon before path; app handles optional spaces).

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
- Per-prompt: `@t:L2` in any user message overrides to 2048 for the request (last one wins).
- If the response is truncated (hits the limit), a warning is printed: "Warning: Response truncated due to max_tokens limit!"

Higher levels allow longer responses but may increase API costs/latency.

### Auto File Requests
Enabled with `--auto-request-files` (or `-a`). This allows Grok to request files from your project directory (current working directory) if it needs them to answer a query better.

- Grok must respond with **exactly** "GROK REQUESTS FILES: relative/path1, relative/path2" (and nothing else).
- Paths must be relative (e.g., `src/main.rs`, not `/absolute/path` or `../outside`). Supports multiple paths, directories, or globs (e.g., `src/*.rs`).
- The utility validates paths (must stay within the project; blocks traversal).
- If valid, it appends a visible note to the last "USER PROMPT:" in the chat file, like:
  ```
  \n\nGROK REQUESTED FILES:\n@f:src/main.rs\n@f:Cargo.toml\n
  ```
- It then immediately re-processes the file, expanding the placeholders so Grok sees the contents in the next API call.
- This chains automatically until Grok provides a normal response.
- Invalid requests are treated as normal responses (not re-queried).
- Security: Requests outside the project are ignored. Disabled by default.

Example:
- User prompt: "What's in my project's main file?"
- Grok requests: GROK REQUESTS FILES: src/main.rs
- App appends to prompt and re-queries with file contents included.

### Auto-Increase Max Tokens
Enabled with `--auto-increase-max-tokens` (or `-i`). When a response is truncated (finish_reason: "max_tokens" or "length"), the utility automatically increments the max_tokens level (from the current prompt's level or default) and re-queries with the same messages but higher max_tokens (e.g., from L3 to L4). This chains until a non-truncated response or L5 is reached. If still truncated at L5, appends with a warning.

Retries are handled in-memory (no file changes until final response). Console shows retry attempts (e.g., "Response truncated. Retrying with L4 (8192 tokens)").

This feature works independently but can chain with auto file requests.

## Notes
- **Polling**: Checks every 1 second; includes a 500ms debounce after detection to handle file saves.
- **File Format**: Must use exact markers ("USER PROMPT:" and "GROK RESPONSE:") on their own lines. Content follows until the next marker.
- **API Model**: Defaults to "grok-4" with temperature=1.0; customizable.
- **Errors**: API failures (e.g., invalid key, timeouts) print to console and play a warning sound. Check logs for details.
- **Sounds**: Bundled MP3 chime for success; generated descending tones for warnings. Disable by removing `rodio` calls if desired.
- **Limitations**: No multi-user support; single-threaded polling. API rate limits/costs apply (check xAI docs).
- **Contributing**: Open issues/PRs on the repository.

For questions, see the in-app help (`--help`) or source code. Enjoy chatting with Grok! 🚀
