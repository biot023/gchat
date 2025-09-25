Can you summarise this project for me from these files:
- @f:src/
- @f:README.md
?

GROK RESPONSE:
### Project Summary: gchat - Interactive Grok AI Chat Tool

**Overview**:  
`gchat` is a Rust-based command-line utility that enables seamless, file-based conversations with Grok AI (from xAI). Instead of a web interface or direct CLI prompts, it monitors a Markdown chat file (default: `./gchat.md`) for changes, processes user prompts (marked by "USER PROMPT:"), sends them to the Grok API along with conversation history, and appends AI responses (marked by "GROK RESPONSE:"). This is ideal for developers or writers who prefer editing text files in editors like Vim or VS Code. The tool polls the file every second, supports audio feedback (chimes for success, warning tones for errors), and includes advanced features like placeholders for embedding file contents or adjusting AI parameters.

The project consists of a single Rust binary (built from `src/main.rs`) with dependencies like `clap` for CLI parsing, `reqwest` for API calls, `regex` for placeholder handling, `rodio` for sounds, and optional tools like `ripgrep` (rg) and `fd-find` (fd). It requires an API key from x.ai and can be configured via CLI flags or a TOML config file (`~/.config/gchat/config.toml`) supporting multiple profiles (e.g., different models or token limits).

**Core Functionality**:
- **File Monitoring**: Runs indefinitely, detecting modifications to the chat file. On startup or changes, it parses the file into user/assistant message pairs, checks for a non-empty latest "USER PROMPT:", and queries the Grok API.
- **API Interaction**: Sends requests to `https://api.x.ai/v1/chat/completions` with configurable parameters (model, max_tokens, temperature, timeout). Handles retries for truncation (e.g., if responses hit token limits) and errors gracefully, logging details via `env_logger`.
- **Message Parsing and Expansion**: Processes Markdown with specific markers. User prompts can include placeholders:
  - `@f:path` (file/glob/directory contents, recursively if dir).
  - `@d:path` (directory tree listing).
  - `@t:L<level>` (override max_tokens, e.g., `@t:L3` for 4096 tokens).
  - `@p:<float>` (override temperature, e.g., `@p:0.5`).
  Placeholders expand in-place before sending to API, with error handling for missing/invalid files.
- **Response Handling**: Appends AI output to the file, adds a new "USER PROMPT:" section, and plays feedback sounds. Includes chaining for advanced features (see below).

**Key Features** (from Code and README):
- **Audio Feedback**: Success chime (MP3) on completion; synthetic warning tones on errors. Uses `rodio` and bundled media files (`media/thinking.mp3`, `media/chime.mp3`).
- **Configurability**: CLI overrides defaults; TOML config with profiles (e.g., `[default]` vs. `[x]` for high-token reasoning). Supports auto-request files, auto-increase tokens, RG/FD commands.
- **Token Management**: Max tokens via levels (L0=512 to L12=~2M tokens, capped at L12). Auto-increase retries truncated responses by ramping up levels.
- **Safety and Validation**: Paths restricted to project directory (no absolute/parent traversal). Commands (RG/FD) use whitelists to prevent injection/shell exploits.
- **Optional Advanced Capabilities**:
  - **Auto File Requests** (flag: `--auto-request-files`): Grok can request files via "GROK REQUESTS FILES: ..." responses; tool appends placeholders and re-processes the file.
  - **RG Commands** (flag: `--allow-rg-commands`): Grok runs safe ripgrep searches (e.g., "GROK RUNS RG: rg -i 'fn' --glob '*.rs'"); output appended and re-queried.
  - **FD Commands** (flag: `--allow-fd-commands`): Similar for fd-find (e.g., "GROK RUNS FD: fd --type f"); useful for file discovery.
  - **File Writes** (flag: `--allow-file-writes`): Grok can overwrite project files via "GROK WRITES TO FILE: ..." responses, validated against user-prompted `@w:` placeholders.
- **Logging and Debugging**: Debug logs show expanded messages/API requests. Handles timeouts (default 600s) and output limits (e.g., 50KB for RG/FD).

**Installation and Usage**:
- **Prerequisites**: Rust 1.70+, API key (`export XAI_API_KEY=...`), optional audio libs (e.g., `libasound2-dev` on Linux), rg/fd tools.
- **Build**: `cargo build --release`.
- **Run**: `cargo run --release -- [options]`, e.g., `cargo run -- -f chat.md -t L3 -a -r -d`.
- **Workflow**: Create/edit chat file (app initializes if missing). Add prompts under "USER PROMPT:", save, and wait for appends. Tool prints status (e.g., "Grok is thinking...").

**Limitations and Notes**:
- Single-threaded polling (1s intervals, 500ms debounce).
- Security-first: All expansions/commands validated to project root.
- API costs/limits apply; model defaults to "grok-code-fast-1".
- Suited for code assistance, as shown in example prompts embedding project files (`@f:./src`, `@f:./Cargo.toml`).
- Disabled by default: file requests, RG/FD, writes—enable via flags/profiles.

Overall, `gchat` is a developer-friendly wrapper for Grok, blending file-editing workflows with AI capabilities while prioritizing safety and extensibility. It's mature, with robust error handling and chaining for complex interactions.

USER PROMPT:
Brilliant, thank you. Now could you please use that understanding to write a new, very-detailed README to the file @w:tmp/README-2.md

GROK RESPONSE:
Generated and overwrote content in tmp/README-2.md.
(Full content saved to the file; check it there.)

USER PROMPT:

