USER PROMPT:
You are a Rust, Grok 4 API, `grok-code-fast-1` and documentation expert.
You are assisting me in developing a Grok 4 chat utility.
The project code files are here: @d:./src
The project's README is: @f:README.md
The project's full code is: @f:src/

Please could you read the full code and readme and then regenerate a full readme me file and write it to @w:tmp/README.md

GROK RESPONSE:
GROK WRITES TO FILE: tmp/README.md
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

This tool is ideal for users who prefer editing a file in their favorite text editor (e.g., VS Code, Vim) rather than using a web interface or CLI prompt. It supports placeholders for including file contents, directory listings, per-prompt token limits, and temperature settings. Additionally, Grok can safely run ripgrep (rg) searches, fd-find (fd) commands on the project, or write generated content to files.

## Features
- **File Watching**: Polls the chat file (default: `./gchat.md`) every 1 second. Processes changes automatically.
- **Conversation History**: Builds and sends the full history as a list of user/assistant messages.
- **Placeholders in Prompts**:
  - `@f:path`: Includes the contents of a file, glob pattern (e.g., `./*.rs`), or entire directory (recursively). Note: No space after `@f` in the placeholder (e.g., `@f:./src/main.rs`), though the app can handle optional spaces.
  - `@d:path`: Includes a tree listing of a directory's contents (files and subdirs).
  - `@t:L<level>`: Sets the `max_tokens` for that specific prompt (e.g., `@t:L3` for 4096 tokens). Overrides the default; the last one across all user messages in history wins.
  - `@p:<value>`: Sets the `temperature` for that specific prompt (e.g., `@p:0.9`). Overrides the default; the last one across all user messages in history wins. Value is a float (e.g., 0.0 to 2.0).
  - `@w:path`: Requests Grok to generate and provide full content for the specified file (e.g., `@w:src/main.rs`). If enabled, Grok can respond with a special format to overwrite the file safely.
- **Audio Feedback**: Chime on success, warning tones on failure (requires audio dependencies for `rodio`). Includes a "thinking" sound during API calls.
- **Logging**: Configure via `RUST_LOG` environment variable (e.g., `RUST_LOG=debug` for detailed output, including API requests/responses).
- **Truncation Handling**: Warns if the API response is truncated due to token limits. Optional auto-increase feature to retry with higher limits.
- **Initial Processing**: On startup, processes any pending user prompt in the file.
- **Auto File Requests**: Optional feature (enabled with `--auto-request-files` or `-a`). Allows Grok to request files from your project directory if needed to answer queries. Grok responds in a specific format ("GROK REQUESTS FILES: relative/path1, relative/path2"), and the utility automatically appends placeholders (e.g., `@f:src/main.rs`) to the last user prompt, then re-queries the API with the contents included. This chains until a normal response is received. Paths are validated to stay within the project directory (no absolute paths or parent traversal). Supports globs and directories if requested.
- **Auto-Increase Max Tokens**: Optional feature (enabled with `--auto-increase-max-tokens` or `-i`). Automatically retries truncated responses with incrementally higher `max_tokens` levels (up to L12) until non-truncated or max is reached.
- **Safe Ripgrep (RG) Commands**: Optional feature (enabled with `--allow-rg-commands` or `-r`). Allows Grok to run safe ripgrep searches on the project for grep-like context. Grok responds with "GROK RUNS RG: rg <safe-args>", e.g., "GROK RUNS RG: rg -i 'fn main' --glob '**/*.rs' --line-number". The utility validates, executes in the project root, appends output to the chat file, and chains. Uses a whitelist of flags; forbids dangerous ones.
- **Safe fd-find (FD) Commands**: Optional feature (enabled with `--allow-fd-commands` or `-d`). Allows Grok to run safe fd searches for file/directory discovery. Grok responds with "GROK RUNS FD: fd <safe-args>", e.g., "GROK RUNS FD: fd --type f --glob '*.md'". The utility validates, executes, appends output, and chains similarly to RG.
- **Safe File Writes**: Optional feature (enabled with `--allow-file-writes` or `-w`). Allows Grok to generate and overwrite project files based on user requests via `@w:path` placeholders. Grok responds with a special format ("GROK WRITES TO FILE: relative/path\n\n[content]"), and the utility validates the path against the user's placeholder before writing. Paths are restricted to the project directory; overwrites the entire file.
- **Profiles**: Support for multiple configurations via `~/.config/gchat/config.toml` using named TOML tables (e.g., [default], [x]). Select with `--profile` or `-p`.

## Installation

### Prerequisites
- **Rust**: Install from [rustup.rs](https://rustup.rs/). Requires Rust 1.70+.
- **Audio Dependencies** (for sounds): On Linux, install `libasound2-dev` and `pkg-config` (e.g., `sudo apt install libasound2-dev pkg-config`). On macOS/Windows, it should work out-of-the-box with `rodio`.
- **Grok API Key**: Sign up at [x.ai](https://x.ai) and get your API key.
- **Optional Tools**: For full functionality, install `ripgrep` (rg) and `fd-find` (fd) if you plan to use RG/FD commands (check your package manager).
- **Media Files**: For audio, ensure `media/chime.mp3` and `media/thinking.mp3` are in the project root (or handle missing files via errors).

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
- `shell-words` for safe command parsing.
- `rodio` for audio feedback.
- `log` and `env_logger` for logging.
- `toml` for config profiles.

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

3. **Optional: Customize Defaults**:
### Profiles in Config
The global config (`~/.config/gchat/config.toml`) supports **profiles** for different setups (e.g., models, token limits). Use named TOML tables:

```toml
[default]  # Or any name; this is used if no -p flag
chat_file = "./gchat.md"
max_tokens = "L3"
temperature = 1.0
model = "grok-code-fast-1"
api_timeout = 600
auto_request_files = false
auto_increase_max_tokens = false
allow_rg_commands = false
allow_fd_commands = false
allow_file_writes = false

[x]  # Example profile for high-token reasoning
chat_file = "./gchat.md"
max_tokens = "L12"  # ~2M tokens
temperature = 1.0
model = "grok-4-fast-reasoning"
api_timeout = 1200
auto_request_files = true
auto_increase_max_tokens = true
allow_rg_commands = true
allow_fd_commands = true
allow_file_writes = true
```

If no profile is specified, it falls back to `[default]` or the first profile. Legacy single-config format is supported for backward compatibility.

4. **Run the App**:
   ```
   cargo run -- [options]
   ```
   The app runs indefinitely until stopped (e.g., Ctrl+C). It creates the chat file if it doesn't exist.

## Usage

### Command-Line Options
Use `cargo run -- --help` for full details. Key options:
- `-f, --chat-file <PATH>`: Path to the chat file (default: `./gchat.md`).
- `-t, --max_tokens <LEVEL>`: Default max tokens level (default: `L3` for 4096 tokens). Can be overridden per-prompt with `@t`. See "Token Levels" below for details.
- `-P, --temperature <FLOAT>`: Default temperature (default: 1.0). Can be overridden per-prompt with `@p`.
- `-m, --model <STRING>`: The Grok model to call (default: `grok-code-fast-1`).
- `-T, --api-timeout <SECONDS>`: API request timeout (default: 600 seconds).
- `-a, --auto-request-files`: Enable Grok to automatically request and include project files if needed (default: false). See "Auto File Requests" below for details.
- `-i, --auto-increase-max-tokens`: Automatically increase max_tokens level on truncation (up to L12) by re-querying (default: false). See "Auto-Increase Max Tokens" below for details.
- `-r, --allow-rg-commands`: Allow Grok to run safe ripgrep commands on the project (default: false). See "RG Commands" below for details.
- `-d, --allow-fd-commands`: Allow Grok to run safe fd commands on the project (default: false). See "FD Commands" below for details.
- `-w, --allow-file-writes`: Allow Grok to write generated content to project files via special responses (default: false). See "File Writes" below for details.
- `-p, --profile <NAME>`: Load a named profile from config.toml (e.g., `-p default` or `-p x`). Overrides defaults.

Example:
```
cargo run -- -f mychat.md -t L3 -P 0.8 -m grok-code-fast-1 -T 300 -a -i -r -d -w -p x
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
- While processing, it prints "Grok is thinking..." and plays a thinking sound. On completion: "Grok has thought." and plays a chime.
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

- **File Write Request (`@w:path`)**:
  - Requests Grok to generate full content for the file (e.g., `@w:tmp/newfile.rs`).
  - Only effective if `--allow-file-writes` is enabled. Grok must respond in the exact format to trigger the write.
  - Paths must be relative; validated against project directory. Multiple `@w:` in a prompt are supported (Grok writes to all matching ones).

Placeholders are case-sensitive and must be formatted exactly (e.g., no space after `@f`, colon before path; app handles optional spaces).

### Token Levels (L* Parameters)
The `--max-tokens` option and `@t` placeholder use "L" levels to specify `max_tokens` (the maximum tokens in the API response). Levels are powers of 2 starting from 512:

- **L0**: 512 tokens
- **L1**: 1024 tokens
- **L2**: 2048 tokens
- **L3**: 4096 tokens
- **L4**: 8192 tokens
- **L5**: 16384 tokens
- **L6**: 32768 tokens
- **L7**: 65536 tokens
- **L8**: 131072 tokens
- **L9**: 262144 tokens
- **L10**: 524288 tokens
- **L11**: 1048576 tokens
- **L12**: 2097152 tokens (maximum; higher levels are capped at L12)

### Auto File Requests
Enabled with `--auto-request-files` (or `-a`). This allows Grok to request files from your project directory (current working directory) if it needs them to answer queries better.

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

### RG Commands
Enabled with `--allow-rg-commands` (or `-r`). This allows Grok to run safe ripgrep (rg) commands on the project for grep-like searches.

- Grok must respond with **exactly** "GROK RUNS RG: rg <safe-args>" (and nothing else).
- Commands must start with `rg ` and use only whitelisted flags (e.g., `-i`, `-n`, `--glob`, `--before-context`).
- Forbidden: Shell metachars (`|`, `>`, `&`, etc.), absolute paths, traversal (`../`).
- The utility parses, runs in the project root, captures output (limited to 50KB), and appends it to the chat file like:
  ```
  \n\nGROK RAN RG: command\n```\noutput\n```\n
  ```
- Then chains to re-query with the appended output.
- Security: Whitelist prevents execution risks. Disabled by default. Requires `ripgrep` installed.

Example:
- Grok request: GROK RUNS RG: rg -i "fn main" --glob "**/*.rs" --line-number
- Output appended and re-queried.

### FD Commands
Enabled with `--allow-fd-commands` (or `-d`). This allows Grok to run safe fd-find (fd) commands on the project for file/directory searches.

- Grok must respond with **exactly** "GROK RUNS FD: fd <safe-args>" (and nothing else).
- Commands must start with `fd ` and use only whitelisted flags (e.g., `--type`, `--glob`, `--max-depth`).
- Forbidden: Shell metachars, absolute paths, traversal.
- Utility parses, runs, appends output (e.g., "GROK RAN FD: command\n```\noutput\n```\n"), and chains.
- Useful for locating files by type/name. Disabled by default. Requires `fd-find` installed.

Example:
- Grok request: GROK RUNS FD: fd --type f --glob "*.md"
- Finds all Markdown files.

### File Writes
Enabled with `--allow-file-writes` (or `-w`). This allows Grok to generate and overwrite files in the project based on user requests.

- Include `@w:relative/path` in your "USER PROMPT:" to request content generation for that file (e.g., `@w:src/lib.rs`).
- Grok must respond with **exactly** "GROK WRITES TO FILE: relative/path\n\n[full raw content]" (and nothing else).
- The utility validates the path matches your `@w:` placeholder, is relative, and within the project (no traversal or absolutes).
- If valid, it overwrites the entire file with the provided content and appends a confirmation to the chat file.
- Multiple writes in one response are supported if paths match multiple `@w:` placeholders.
- Security: Path must exactly match user's request; disabled by default to prevent accidental overwrites.

Example:
- User prompt: "Generate a new Rust module. @w:src/module.rs"
- Grok responds with write format → File `src/module.rs` is overwritten, confirmation appended.

### Auto-Increase Max Tokens
Enabled with `--auto-increase-max-tokens` (or `-i`). When a response is truncated (finish_reason: "max_tokens" or "length"), the utility automatically increments the max_tokens level (from the current prompt's level or default) and re-queries with the same messages but higher max_tokens (e.g., from L3 to L4). This chains until a non-truncated response or L12 is reached. If still truncated at L12, appends with a warning.

Retries are handled in-memory (no file changes until final response). Console shows retry attempts (e.g., "Response truncated. Retrying with L4 (8192 tokens)").

This feature works independently but can chain with auto file requests, RG, FD, or file writes.

## Notes
- **Polling**: Checks every 1 second; includes a 500ms debounce after detection to handle file saves.
- **File Format**: Must use exact markers ("USER PROMPT:" and "GROK RESPONSE:") on their own lines. Content follows until the next marker.
- **API Model**: Defaults to "grok-code-fast-1" with temperature=1.0; customizable.
- **Errors**: API failures (e.g., invalid key, timeouts) print to console and play a warning sound. Check logs for details.
- **Sounds**: Bundled MP3 chime and thinking sounds for success/processing; generated descending tones for warnings. Errors are logged if media files are missing.
- **Limitations**: No multi-user support; single-threaded polling. API rate limits/costs apply (check xAI docs).
- **Security**: File requests, writes, RG, and FD are restricted to the project root and use whitelists to prevent unsafe operations. Enable only as needed. Profiles load from a standard config dir.
- **Contributing**: Open issues/PRs on the repository.

For questions, see the in-app help (`--help`) or source code. Enjoy chatting with Grok! 🚀

USER PROMPT:


GROK RESPONSE:
GROK WRITES TO FILE: tmp/README.md
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

This tool is ideal for users who prefer editing a file in their favorite text editor (e.g., VS Code, Vim) rather than using a web interface or CLI prompt. It supports placeholders for including file contents, directory listings, per-prompt token limits, and temperature settings. Additionally, Grok can safely run ripgrep (rg) searches or fd-find (fd) commands on the project to gather context, or generate and write content to files.

## Features
- **File Watching**: Polls the chat file (default: `./gchat.md`) every 1 second. Processes changes automatically.
- **Conversation History**: Builds and sends the full history as a list of user/assistant messages.
- **Placeholders in Prompts**:
  - `@f:path`: Includes the contents of a file, glob pattern (e.g., `./*.rs`), or entire directory (recursively). Note: No space after `@f` in the placeholder (e.g., `@f:./src/main.rs`), though the app can handle optional spaces.
  - `@d:path`: Includes a tree listing of a directory's contents (files and subdirs).
  - `@t:L<level>`: Sets the `max_tokens` for that specific prompt (e.g., `@t:L3` for 4096 tokens). Overrides the default; the last one across all user messages in history wins.
  - `@p:<value>`: Sets the `temperature` for that specific prompt (e.g., `@p:0.9`). Overrides the default; the last one across all user messages in history wins. Value is a float (e.g., 0.0 to 2.0).
  - `@w:path`: Requests Grok to generate full content for the specified file path (e.g., `@w:src/main.rs`). Only effective if file writes are enabled; triggers safe overwriting.
- **Audio Feedback**: Chime on success, warning tones on failure (requires `rodio`). Includes a "thinking" sound during API processing.
- **Logging**: Configure via `RUST_LOG` environment variable (e.g., `RUST_LOG=debug` for detailed output, including API requests/responses).
- **Truncation Handling**: Warns if the API response is truncated due to token limits. Optional auto-increase to retry with higher limits (up to L12).
- **Initial Processing**: On startup, processes any pending user prompt in the file.
- **Auto File Requests**: Optional ( `--auto-request-files` or `-a` ). Grok can request files via "GROK REQUESTS FILES: path1, path2", triggering automatic inclusion via placeholders and re-query.
- **Auto-Increase Max Tokens**: Optional ( `--auto-increase-max-tokens` or `-i` ). Retries truncated responses with higher token limits until non-truncated or L12 max.
- **Safe Ripgrep (RG) Commands**: Optional ( `--allow-rg-commands` or `-r` ). Grok can run validated rg searches via "GROK RUNS RG: rg <args>", appending output and chaining.
- **Safe fd-find (FD) Commands**: Optional ( `--allow-fd-commands` or `-d` ). Grok can run validated fd searches via "GROK RUNS FD: fd <args>", appending output and chaining.
- **Safe File Writes**: Optional ( `--allow-file-writes` or `-w` ). Grok can overwrite files via "GROK WRITES TO FILE: path\n\ncontent" if matching user `@w:` placeholders.
- **Profiles**: Multiple configs in `~/.config/gchat/config.toml` using TOML tables (e.g., [default], [x]). Select via `--profile` or `-p`.

## Installation

### Prerequisites
- **Rust**: Install from [rustup.rs](https://rustup.rs/). Requires Rust 1.70+.
- **Audio Dependencies** (for sounds): On Linux, install `libasound2-dev` and `pkg-config` (e.g., `sudo apt install libasound2-dev pkg-config`). macOS/Windows: works out-of-the-box with `rodio`.
- **Grok API Key**: Obtain from [x.ai](https://x.ai).
- **Optional Tools**: `ripgrep` (rg) and `fd-find` (fd) for search features.
- **Media Files**: Optional MP3s in `./media/` for custom sounds (chime.mp3, thinking.mp3); falls back to generated tones if missing.

### Building the Project
1. Clone the repository:
   ```
   git clone <repository-url>
   cd <repository-dir>
   ```
2. Build:
   ```
   cargo build --release
   ```
   Executable: `target/release/gchat`.

   Run directly:
   ```
   cargo run --release -- [options]
   ```

### Dependencies
- `clap`: CLI parsing.
- `reqwest` + `tokio`: API calls.
- `serde`: JSON.
- `regex`, `walkdir`, `glob`: Placeholder expansion.
- `shell-words`: Safe command parsing.
- `rodio`: Audio.
- `log` + `env_logger`: Logging.
- `toml`: Config profiles.

Pulled via `Cargo.toml`.

## Setup
1. **API Key**:
   ```
   export XAI_API_KEY=your-api-key-here
   ```
   Add to `~/.bashrc` for persistence.

2. **Logging**:
   ```
   export RUST_LOG=debug
   ```
   Levels: `info` (default), `debug` (detailed).

3. **Config Profiles** (`~/.config/gchat/config.toml`):
   ```toml
   [default]
   chat_file = "./gchat.md"
   max_tokens = "L3"
   temperature = 1.0
   model = "grok-code-fast-1"
   api_timeout = 600
   auto_request_files = false
   auto_increase_max_tokens = false
   allow_rg_commands = false
   allow_fd_commands = false
   allow_file_writes = false

   [high-reasoning]
   max_tokens = "L12"
   model = "grok-4-fast-reasoning"
   auto_request_files = true
   allow_file_writes = true
   ```
   CLI `-p high-reasoning` selects profile. Falls back to [default] or first. Supports legacy single-config.

4. **Run**:
   ```
   cargo run -- [options]
   ```
   Runs until Ctrl+C; creates chat file if needed.

## Usage

### Command-Line Options
`cargo run -- --help` for details.
- `-f, --chat-file <PATH>`: Chat file (default: `./gchat.md`).
- `-t, --max_tokens <LEVEL>`: Default tokens (default: L3=4096).
- `-P, --temperature <FLOAT>`: Default temperature (default: 1.0).
- `-m, --model <STRING>`: Model (default: `grok-code-fast-1`).
- `-T, --api-timeout <SECONDS>`: Timeout (default: 600s).
- `-a, --auto-request-files`: Enable auto file requests.
- `-i, --auto-increase-max-tokens`: Auto-retry on truncation.
- `-r, --allow-rg-commands`: Enable RG.
- `-d, --allow-fd-commands`: Enable FD.
- `-w, --allow-file-writes`: Enable file writes.
- `-p, --profile <NAME>`: Config profile (e.g., `-p high-reasoning`).

Example:
```
cargo run -- -f mychat.md -t L4 -P 0.8 -m grok-code-fast-1 -a -i -r -d -w -p high-reasoning
```

### Basic Workflow
1. Start app: Creates `./gchat.md` with "USER PROMPT:" if needed.
2. Edit file: Add prompt under "USER PROMPT:", save.
3. App detects, processes (expands placeholders, calls API), appends "GROK RESPONSE:", adds new "USER PROMPT:".
4. Repeat.

Example `gchat.md`:
```
USER PROMPT:
Hello, Grok! @t:L2 @p:0.5

GROK RESPONSE:
Hello! How can I help?

USER PROMPT:
What's the meaning of life? @f:./src/main.rs
```
- Processes only non-empty last "USER PROMPT:".
- Startup: Processes pending prompts.
- During: "Grok is thinking..." + sound.
- After: "Grok has thought." + chime.
- Errors: Warning sound + console/log.

### Placeholders in User Prompts
Expanded only in "USER PROMPT:" before API send; removed post-expansion.

- **@f:path** (Files/Globs/Dirs):
  - File: `@f:./file.rs` → "Contents of ./file.rs:\n```\ncontent\n```\n".
  - Glob: `@f:./src/*.rs` → Sorted matching files.
  - Dir: `@f:./src` → All recursive files, sorted.
  - Errors: Warn, unexpanded (e.g., "File not found").

- **@d:path** (Dir Tree):
  - `@d:./src` → "Contents of directory ./src:\n```\ntree listing\n```\n".
  - Recurses; empty dirs noted, not error.

- **@t:L<level>**: Overrides max_tokens (last wins).
- **@p:<value>**: Overrides temperature (last wins; 0.0-2.0 typical).
- **@w:path**: Requests file generation/overwrite (validated if enabled).

Case-sensitive; optional spaces after colon handled.

### Token Levels
`max_tokens` = 512 * 2^level:
- L0: 512
- L1: 1024
- L2: 2048
- L3: 4096
- L4: 8192
- L5: 16384
- L6: 32768
- L7: 65536
- L8: 131072
- L9: 262144
- L10: 524288
- L11: 1048576
- L12: 2097152 (max; capped)

### Auto File Requests (`-a`)
Grok: "GROK REQUESTS FILES: src/main.rs, Cargo.toml" → Appends `@f:` to prompt, re-queries. Validates relative/within-project/no-traversal. Chains until normal response. Disabled default.

### RG Commands (`-r`)
Grok: "GROK RUNS RG: rg -i 'error' --glob '**/src/*.rs' -n" → Validates whitelist (e.g., -i, --glob, -n; no shell chars), runs in project root (30s timeout, 50KB limit), appends output, chains. Requires rg. Disabled default.

### FD Commands (`-d`)
Grok: "GROK RUNS FD: fd --type f --glob '*.rs' --max-depth 2" → Validates whitelist (e.g., --type, --glob; no shell chars), runs (30s timeout, 50KB limit), appends, chains. Requires fd. Disabled default.

### File Writes (`-w`)
Prompt: "Generate README. @w:tmp/README.md"  
Grok: "GROK WRITES TO FILE: tmp/README.md\n\n[raw content]" → Validates path matches `@w:`, relative/within-project/no-traversal, overwrites file, appends confirmation. Multiple supported. Disabled default; security-focused.

### Auto-Increase Max Tokens (`-i`)
On truncation (finish_reason: max_tokens/length), increments level (e.g., L3→L4), re-queries in-memory until complete or L12. Warns at max. Chains with other features. Disabled default.

## Notes
- **Polling**: 1s interval + 500ms debounce for saves.
- **Format**: Exact "USER PROMPT:" / "GROK RESPONSE:" on lines; content until next.
- **Errors**: API/key/timeout → console + warning sound; logs detailed.
- **Sounds**: MP3 if in `./media/`; else generated (SineWave tones). Non-blocking.
- **Limits**: Single-threaded; API rates/costs apply (see xAI docs).
- **Security**: All features (requests, writes, commands) restrict to project root, whitelist flags, no traversal/absolutes. Enable judiciously.
- **Contributing**: Issues/PRs welcome.

See `--help` or source. Enjoy! 🚀

USER PROMPT:

