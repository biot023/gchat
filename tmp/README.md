# Grok 4 Chat Utility

A lightweight Rust command-line utility for interacting with the Grok 4 API. This tool allows users to engage in conversational chats with Grok models, specifically optimized for the `grok-code-fast-1` model. It supports sending messages, maintaining conversation history, and streaming responses for a natural chat experience.

## Features

- **Interactive Chat Mode**: Start a conversation and exchange messages in real-time.
- **API Integration**: Seamlessly connects to the xAI Grok API using official endpoints.
- **Conversation History**: Maintains context across multiple turns for coherent responses.
- **Streaming Support**: Receives responses as they generate, mimicking a live chat.
- **Error Handling**: Robust handling of API errors, rate limits, and network issues.
- **Configuration**: Easy setup via environment variables for API keys and model selection.
- **Rust Best Practices**: Written in idiomatic Rust with async support using Tokio.

## Prerequisites

- Rust toolchain (version 1.75 or later) installed via [rustup](https://rustup.rs/).
- An xAI API key from [xAI Console](https://console.x.ai/).
- Optional: `cargo-edit` for adding dependencies easily.

## Installation

1. Clone or create the project directory:
   ```bash
   mkdir grok-chat-utility
   cd grok-chat-utility
   cargo init
   ```

2. Add dependencies to `Cargo.toml`:
   ```toml
   [dependencies]
   tokio = { version = "1", features = ["full"] }
   reqwest = { version = "0.11", features = ["json", "stream"] }
   serde = { version = "1.0", features = ["derive"] }
   serde_json = "1.0"
   clap = { version = "4.0", features = ["derive"] }
   anyhow = "1.0"
   ```

3. Set your xAI API key as an environment variable:
   ```bash
   export XAI_API_KEY="your_api_key_here"
   ```

## Usage

Build and run the utility:

```bash
cargo build
cargo run -- --help
```

### Command-Line Options

The utility uses Clap for argument parsing. Basic usage:

```bash
# Start an interactive chat
cargo run -- chat

# Send a single message (non-interactive)
cargo run -- send "Hello, Grok! Explain Rust ownership."

# Specify a different model (default: grok-code-fast-1)
cargo run -- chat --model "grok-4"
```

- `--model <MODEL>`: Select the Grok model (e.g., `grok-code-fast-1`, `grok-4`).
- `--api-url <URL>`: Override the default API endpoint (defaults to `https://api.x.ai/v1`).
- `--max-tokens <N>`: Limit response length (default: 1024).

### Interactive Mode

In chat mode:
- Type your messages and press Enter.
- Responses stream in real-time.
- Type `/quit` or `/exit` to end the session.
- Type `/clear` to reset conversation history.

Example session:
```
> Hello, what is Rust?
Grok: Rust is a systems programming language that runs at a similar speed to C...
> Tell me more about borrowing.
Grok: In Rust, borrowing is a key part of its ownership system...
```

## Project Structure

- `src/main.rs`: Entry point with CLI parsing and chat loop.
- `src/api.rs`: Handles API requests, authentication, and streaming.
- `src/chat.rs`: Manages conversation state and user input.
- `Cargo.toml`: Dependencies and build configuration.

## API Integration Details

This utility uses the xAI Grok API endpoints:
- **Chat Completions**: `POST /chat/completions` for generating responses.
- Authentication: Bearer token via `XAI_API_KEY` environment variable.
- Request Format: JSON with `model`, `messages` array, and optional `stream: true`.

Example API request payload:
```json
{
  "model": "grok-code-fast-1",
  "messages": [{"role": "user", "content": "Your message"}],
  "stream": true,
  "max_tokens": 1024
}
```

For full API documentation, refer to the [xAI API Docs](https://docs.x.ai/).

## Configuration

All sensitive data is loaded from environment variables:
- `XAI_API_KEY`: Required for authentication.
- `GROK_MODEL`: Optional override for default model (`grok-code-fast-1`).

## Testing

Run unit tests:
```bash
cargo test
```

Integration tests (requires API key):
```bash
cargo test -- --test-threads=1
```

## Contributing

1. Fork the repository.
2. Create a feature branch: `git checkout -b feature/new-feature`.
3. Commit changes: `git commit -am 'Add new feature'`.
4. Push to the branch: `git push origin feature/new-feature`.
5. Open a Pull Request.

Ensure code follows Rust style guidelines (run `cargo fmt` and `cargo clippy`).

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Support

- Report issues on the GitHub repository.
- For API-related questions, check xAI documentation or community forums.

Happy chatting with Grok! 🚀