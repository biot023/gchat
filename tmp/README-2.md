# gchat: Interactive Grok AI Chat Tool for Developers

A powerful, file-based Rust utility for seamless conversations with Grok AI (from xAI). Designed for developers, writers, and anyone who prefers editing a Markdown file in their favorite text editor over web interfaces or traditional CLI prompts. `gchat` polls a chat file for changes, expands placeholders for context (files, directories, parameters), sends prompts to the Grok API, and appends responses—complete with audio feedback, optional chaining features, and strict safety checks.

This tool transforms your text editor into a collaborative AI workspace. Edit the chat file to ask questions, include code snippets, or request analyses, and let `gchat` handle the API integration, file expansions, and response management. It's especially suited for coding tasks, where embedding project files or running safe searches provides deep, accurate assistance.

## Table of Contents
- [Overview](#overview)
- [Key Features](#key-features)
- [How It Works](#how-it-works)
- [Installation](#installation)
- [Configuration](#configuration)
- [Usage](#usage)
- [Placeholders and Syntax](#placeholders-and-syntax)
- [Token Levels and Parameters](#token-levels-and-parameters)
- [Advanced Features](#advanced-features)
- [Safety and Security](#safety-and-security)
- [Examples](#examples)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## Overview
`gchat` is a single-binary Rust application that monitors a Markdown chat file (default: `./gchat.md`) for modifications. When a new user prompt is detected (marked by "USER PROMPT:"), it:
1. Parses the file into a conversation history.
2. Expands placeholders in user messages (e.g., `@f:src/main.rs` to include file contents).
3. Sends the history to the Grok API with configurable parameters (model, max_tokens, temperature).
4. Appends the AI response (marked by "GROK RESPONSE:") and a new "USER PROMPT:" section.
5. Provides audio cues: a chime for success, tones for warnings.

The app runs indefinitely, polling every 1 second, making it feel like a live editor session. It supports optional "superpowers" like letting Grok request files, run safe ripgrep (rg) searches, or even write to project files—enabled via flags for security.

Built with async Rust using `tokio`, `reqwest` for API calls, and libraries like `clap` (CLI), `regex` (placeholders), `rodio` (audio), and `walkdir` (directory handling). It prioritizes safety: no shell execution, path traversal blocked, and all expansions within the project directory.

## Key Features
- **File-Based Workflow**: No UI—just edit a Markdown file in your editor (e.g., VS Code, Vim, Neovim). Changes trigger API calls automatically.
- **Placeholder Expansion**: Embed file contents, directory trees, or override AI parameters directly in prompts (e.g., `@f:Cargo.toml` includes the file, `@t:L3` sets max tokens).
- **Conversation History**: Maintains full context by sending all prior user/assistant exchanges.
- **Audio Feedback**: Built-in sounds for status (chime on success, descending tones on errors) using `rodio` and MP3 files.
- **Configurable Parameters**: Override defaults for model, max_tokens (via levels), temperature, timeout, etc., per-prompt or globally.
- **Profiles Support**: Use TOML config with named profiles (e.g., "default" for quick chats, "x" for heavy reasoning).
- **Truncation Handling**: Detects API truncation and optionally auto-retries with higher token limits.
- **Optional Advanced Modes**:
  - Auto-request files: Grok can ask for project files to improve responses.
  - Safe RG/FD Commands: Grok runs whitelisted ripgrep or fd-find searches for context.
  - File Writes: Grok can generate/overwrite files in your project via user-approved placeholders.
- **Safety-First Design**: Validates all paths (relative only, no traversal); whitelists commands; timeouts and output limits prevent abuse.
- **Logging**: Detailed debug logs (set `RUST_LOG=debug`) for requests/responses, expansions, and errors.
- **Debounce and Polling**: 500ms debounce after detection to handle saves; 1s polling for efficiency.

## How It Works
1. **Startup**: Run `gchat` with optional flags. It checks/creates the chat file with an initial "USER PROMPT:" marker. If a prompt exists, it processes immediately.
2. **Polling Loop**: Every 1 second, checks the file's modification time.
3. **Detection**: On change, reads and parses the file using markers ("USER PROMPT:", "GROK RESPONSE:") into a vector of Message structs (role: "user" or "assistant", content: string).
4. **Validation**: Skips if no non-empty user prompt at the end.
5. **Expansion**: Processes placeholders in user messages (only), replacing/removing them. Collects failed paths for warnings.
6. **API Call**: Builds a request with history, sends to Grok, handles retries for truncation or chaining (file requests, RG/FD).
7. **Response Handling**: Appends response to file, plays sound, adds new prompt marker. For special responses (e.g., file requests), appends placeholders and re-processes in a loop.
8. **Termination**: Runs until interrupted (Ctrl+C). No persistence beyond the chat file.

Sounds are non-blocking and use bundled media (`media/thinking.mp3`, `media/chime.mp3`). Warnings generate synthetic tones via `SineWave`.

## Installation
### Prerequisites
- **Rust**: Version 1.70+ (install via [rustup.rs](https://rustup.rs/)).
- **Grok API Key**: Obtain from [x.ai](https://x.ai) and set as `export XAI_API_KEY=your-key-here` (add to `~/.bashrc` or similar for persistence).
- **Audio (Optional but Recommended)**: For Linux, install `libasound2-dev` and `pkg-config` (`sudo apt install libasound2-dev pkg-config`). macOS/Windows: Usually works out-of-the-box with `rodio`.
- **Optional Tools**: `ripgrep` (rg) and `fd-find` (fd) for RG/FD features (install via package manager, e.g., `sudo apt install ripgrep fd-find`).

### Building
1. Clone or download the project: `git clone <repo-url> && cd <project-dir>`.
2. Build: `cargo build --release`.
3. Run: `cargo run --release -- [options]`. Binary at `target/release/gchat`.

## Configuration
Configured via CLI flags, a global TOML file (`~/.config/gchat/config.toml`), or both (CLI overrides config). Profiles allow switching setups (e.g., fast vs. reasoning mode).

### TOML Config File
Located at `~/.config/gchat/config.toml` (create if missing). Supports profiles as TOML tables (e.g., `[default]`, `[x]`). Non-profiled configs (legacy) default to `[default]`.

Example:
```toml
[default]
chat_file = "./gchat.md"
max_tokens = "L3"  # 4096 tokens
temperature = 1.0
model = "grok-code-fast-1"
api_timeout = 600
auto_request_files = false
auto_increase_max_tokens = false
allow_rg_commands = false
allow_fd_commands = false
allow_file_writes = false

[x]  # High-power profile for code reasoning
chat_file = "./gchat.md"
max_tokens = "L12"  # ~2M tokens
temperature = 0.7
model = "grok-4-fast-reasoning"
api_timeout = 1200
auto_request_files = true
auto_increase_max_tokens = true
allow_rg_commands = true
allow_fd_commands = true
allow_file_writes = false
```

### CLI Flags
Run `cargo run --release -- --help` for full list. Key flags:
- `-f, --chat-file <PATH>`: Chat file path (default: `./gchat.md`).
- `-t, --max_tokens <LEVEL>`: Default max_tokens level (default: `L3`, 4096 tokens). Overrides config.
- `-P, --temperature <FLOAT>`: Default temperature (0.0-2.0, default: 1.0).
- `-m, --model <STRING>`: Grok model (default: `grok-code-fast-1`).
- `--api-timeout <SECONDS>`: Request timeout (default: 600).
- `-a, --auto-request-files`: Enable Grok file requests.
- `-i, --auto-increase-max-tokens`: Auto-retry on truncation (up to L12).
- `-r, --allow-rg-commands`: Enable safe RG commands.
- `-d, --allow-fd-commands`: Enable safe FD commands.
- `-w, --allow-file-writes`: Enable file writes via `@w:` placeholders.
- `-p, --profile <NAME>`: Load profile from config (e.g., `-p x`).

If no profile specified, uses `[default]` or first profile. No config? Pure defaults.

## Usage
1. **Setup**: `export XAI_API_KEY=...` and optionally `export RUST_LOG=debug`.
2. **Start**: `cargo run --release -- -a -r -d -w -p x` (enable all features, use profile "x").
3. **Edit Chat File**: Open `./gchat.md`. Add prompts under "USER PROMPT:", include placeholders, save. Tool detects change, processes, appends response.
4. **Interact**: Modify the new "USER PROMPT:" section, repeat. Delete history to shorten contexts for faster processing.

Example `./gchat.md` after processing:
```
USER PROMPT:
Hello, Grok! @f:src/main.rs

GROK RESPONSE:
Hello! I see your main.rs file...

USER PROMPT:
Now analyze the code.
```

- Starts immediately if prompt exists.
- Prints status: "Grok is thinking... (max_tokens: 4096, temperature: 1.0)", then "Grok has thought (X seconds)."
- Errors: Print to console, log details, play warning sound.

## Placeholders and Syntax
Processed only in "USER PROMPT:" sections, expanded/removed before API send. Case-sensitive, format: `@<type>:<value>`. Handles optional spaces (e.g., `@f :path`).

- **File Contents (`@f:path`)**: Includes contents of files/directories/globs.
  - Single file: `@f:src/main.rs` → "Contents of src/main.rs:\n```\n[code]\n```\n".
  - Glob: `@f:src/*.rs` → Sorted list of matching files' contents.
  - Directory: `@f:src/` → Recursive contents of all files (sorted, ignores directories).
  - Errors: Warns with "Failed to read file..." if missing/inaccessible.
- **Directory Tree (`@d:path`)**: Lists directory structure.
  - `@d:src/` → "Contents of directory src:\n```\nsrc/\n  main.rs\n  utils/\n    mod.rs\n```\n". Recursive, depth-indented.
- **Max Tokens (`@t:L<level>`)**: Override max_tokens for prompt (last across messages wins).
  - Example: `@t:L4` (8192 tokens). Levels: L0 (512) to L12 (~2M).
- **Temperature (`@p:<float>`)**: Override temperature (0.0=deterministic to 2.0=creative).
  - Example: `@p:0.5`.
- **Write Paths (`@w:path`)**: Declare allowed write paths for Grok. Must match in responses for security.
  - Example: `@w:generated_output.md`. Grok can only write if path matches.

Failed expansions note paths in warnings, included in prompt to API.

## Token Levels and Parameters
- **Token Levels**: `max_tokens` calculated as 512 * 2^level (capped at L12=524288).
  - L0: 512, L1: 1024, ..., L12: 524288 (~2M tokens).
- **Temperature**: Controls creativity (0.0=fixed, 2.0=random).
- **Model**: "grok-code-fast-1" for coding, "grok-4-fast-reasoning" for complex tasks.
- **Timeout**: API timeout (seconds); default 600.

## Advanced Features
### Auto File Requests
Enable with `-a`. Grok can request files for better answers via "GROK REQUESTS FILES: path1,path2" (exact format). Tool validates (relative, no traversal), appends placeholders to chat file (e.g., `@f:path1\n@f:path2`), re-processes. Chains until normal response. Security: Blocks invalid paths.

### Auto-Increase Max Tokens
Enable with `-i`. On truncation (finish_reason: "max_tokens"), retries with next level (up to L12). In-memory, no file changes until success. Warns if still truncated.

### RG Commands
Enable with `-r`. Grok runs "GROK RUNS RG: rg [whitelisted-args]" (e.g., `rg -i "fn" --glob "**/*.rs" -n`). Tool parses safely (using `shell-words`), executes in project root, limits output to 50KB, appends fenced block to file, re-processes. Whitelist: `-i`, `-n`, `--glob`, `--type`, etc. Forbids metachars, absolutes.

### FD Commands
Enable with `-d`. Similar to RG: "GROK RUNS FD: fd [args]" (e.g., `fd --type f --glob "*.md"`). For file/directory searches. Same safety/limits.

### File Writes
Enable with `-w`. Grok generates files via "GROK WRITES TO FILE: path\n[content]" (must match user's `@w:path` from prompt). Tool overwrites (creates dirs if needed), appends confirmation to chat (but not full content). Validates paths strictly.

Chaining: File requests, RG/FD, or retries can loop multiple times.

## Safety and Security
- **Paths**: All must be relative, within project (no `..` or absolutes). Uses `canonicalize` and `starts_with(cwd)`.
- **Commands**: RG/FD parsed with `shell-words`, whitelisted flags, no shell metachars (`|`, `>`, `&`). Timeouts (30s RG/FD, 600s API) and size limits (50KB output).
- **Writes**: Only if path in user's prompt `@w:` placeholders.
- **Expansions**: Glob/dir recurse only if valid; logs errors.
- **No Execution**: Pure Rust, no `system()` calls. API key from env, not stored.

## Examples
### Basic Chat
```md
USER PROMPT:
What is Rust? @p:0.8

GROK RESPONSE:
Rust is a systems programming language...
```

### Code Analysis
```md
USER PROMPT:
Analyze this code. @f:src/main.rs @d:src/

GROK RESPONSE:
Your main.rs defines... and the directory has...
```

### With RG Request
(Assuming `-r` enabled):
```md
USER PROMPT:
Find all fn main definitions.

GROK RESPONSE:
GROK RUNS RG: rg -i "fn main" --glob "**/*.rs" -n
```
Tool appends output, re-processes.

## Troubleshooting
- **No Response/API Error**: Check API key, internet, model existence. Logs show details (`RUST_LOG=debug`).
- **Placeholder Failures**: Verify paths exist/relative. Warnings list failed paths.
- **Sounds Not Playing**: Check audio drivers; remove `rodio` calls if unwanted.
- **Truncation**: Increase default level or enable `-i`.
- **Config Issues**: TOML parse errors logged; fall back to defaults.
- **Performance**: Long histories slow API; trim file.
- **Timeouts**: Increase `--api-timeout`.

## Contributing
- Issues/PRs: Open on repo.
- Code Style: Use `cargo fmt` and `clippy`.
- Testing: Add tests for parsing/expansion. Requires API key for integration.

## License
[Specify if applicable, e.g., MIT] - Open-source Rust project. No warranty.

Enjoy coding with Grok—edit, expand, iterate! 🚀🤖