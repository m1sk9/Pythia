# AGENTS.md

This file provides guidance to coding agents when working with code in this repository.

## Project Overview

Pythia is a Discord bot that bridges a server and LLM APIs: one thread is one conversation, and the bot keeps no state beyond what Discord holds. v3 is a rewrite on twilight and Tokio (Rust edition 2024, MSRV 1.89). A mention in an allowed guild's channel starts a thread, the conversation continues there without mentions, recent image attachments are sent to vision models as Base64 image parts, and OpenRouter server tools (web search, web fetch, datetime) can be enabled so that answers cite their sources. It is shipped as a distroless Docker image (`ghcr.io/m1sk9/pythia`).

## Common Commands

```bash
cargo build                                    # Debug build
cargo test --verbose                           # Run all tests
cargo test split_message                       # Run tests whose name contains the filter
cargo fmt --all -- --check                     # Check formatting
cargo clippy --all-targets --all-features      # Lint
CONFIG_FILE_PATH=config/config.toml cargo run  # Run (tokens from the environment or .env)
docker build -t pythia:local .                 # Build the release image (cargo-chef → distroless)
docker compose up -d                           # Run ghcr.io/m1sk9/pythia:v3 with ./config/config.toml
```

Coverage (CI uploads it to Codecov) needs nightly, because `rust-toolchain.toml` pins stable: `cargo +nightly llvm-cov --all-features --workspace` (requires `cargo-llvm-cov` and `llvm-tools-preview`).

Live tests are `#[ignore]`d and read `.env`: `cargo test live_ -- --ignored --nocapture` (`OPENROUTER_MODEL` overrides `llm.model`). `live_web_search` runs a billed web search and prints the citations and usage.

## Architecture

The code is split into three layers: **on-event** decides whether a gateway event should start a turn, the **orchestrator** turns a thread into an LLM request and back, and **interact-to** posts the result to Discord.

**Entry flow:** `main.rs` → loads env + TOML config → initializes logging → builds the shared `reqwest::Client` (with `llm.timeout_secs`), looks up model capabilities (images, tools), and builds the OpenRouter client → builds `AppState` (twilight-http client, enabled server tools, channel cache, registry, scheduler, semaphore) and the shard → `gateway.rs` event loop → graceful shutdown on Ctrl-C / SIGTERM

| module | layer | role |
|---|---|---|
| `main.rs` | wiring | config → logging → clients → gateway loop → shutdown |
| `config.rs` | — | env + TOML config |
| `gateway.rs` | on-event | shard loop, cache, trigger decision |
| `thread.rs` | on-event | thread naming, conversation registry, per-thread turn state |
| `context.rs` | orchestrator | Discord messages → chat messages (pure) |
| `attachments.rs` | orchestrator | image download / validation / base64 |
| `llm.rs`, `llm/openrouter.rs` | orchestrator | chat-completions shape and the OpenRouter client |
| `orchestrator.rs` | orchestrator | one conversation turn |
| `reply.rs` | interact-to | message splitting, error embeds, posting |

**Key modules:**

- **`config.rs`** — `EnvConfig` (secrets, via `envy`) and `PythiaConfig` (TOML, via `CONFIG_FILE_PATH`), both stored in `OnceLock`s. The file is deserialized into private `Raw*` structs and then validated into the public types, so required keys (`llm.model`) are reported by their full path and `system_prompt` / `system_prompt_file` are resolved into a single string. `[tools]` enables each OpenRouter server tool individually (all off by default, since searches and fetches are billed per request); `ToolsConfig::server_tools` lists them, with `datetime` in `thread.timezone` and `web_search_engine` / `web_search_mode` / `web_fetch_engine` passed as tool parameters (unset → left out, so OpenRouter's defaults apply; the mode is an engine-specific string). `PythiaConfig::activity_text` resolves `discord.activity_display` (`model` / `version` / `custom` / `disabled`; `custom` requires `activity_custom`, ≤ 128 chars) into the status text. `config/config.toml` is the reference config and is covered by a test.
- **`llm.rs` / `llm/openrouter.rs`** — Provider-neutral, OpenAI-shaped types (`ChatMessage`, `ChatRequest`, `ChatResponse`, `ServerTool`, `Citation`, `LlmError`) and the OpenRouter client. Enabled server tools go in `tools` as `{"type": "openrouter:…"}` (omitted when empty); OpenRouter runs them, so there is no tool-calling loop. `url_citation` annotations become `ChatResponse.citations` (first of each URL), and `web_search_requests` / `tool_calls_executed` are read from `usage.server_tool_use_details` (chat completions) or `usage.server_tool_use`. Request building, response parsing, status → error mapping, and the retry policy (`should_retry`: 429 / 5xx / connection errors, delays 1 s then 3 s, `Retry-After` honoured up to 30 s) are pure functions; only `chat` / `fetch_capabilities` touch the network. A failed `/models` lookup disables images and tools instead of stopping startup; tools are also disabled when the model's `supported_parameters` lacks `tools`. `OpenRouterClient` holds the API key and does not derive `Debug`.
- **`context.rs`** — `build_context` turns thread history (newest first, as Discord returns it) plus the optional starter message into chronological `ChatMessage`s and the id of the newest user message to reply to; messages after it are not sent, so the conversation always ends with that message. It drops other bots, system users, and non-`Regular`/`Reply` messages; labels users as `"{display_name}: {content}"` (nick → global name → username); removes the bot's mention and rewrites other user mentions as `@name`, reusing their speaker label when they posted in the thread; drops the footer (`reply::without_footer`) under the bot's earlier answers so the model does not imitate it; merges consecutive bot posts; renders attachments as `[image: …]` / `[attachment: …]` lines and appends fetched images as image parts after the text (`Content::Parts`); and keeps the newest messages within `context.max_chars`. No user message left → `ContextError::NoUserMessage`. Tests build `Message`s from JSON because the struct has a deprecated field.
- **`attachments.rs`** — Image policy. `images_enabled = attachments.images && the model accepts images` (decided once at startup). `select_images` walks the user messages kept by the context (`context::user_messages_newest_first`), takes PNG / JPEG / WebP / GIF attachments from the newest `attachments.recent_messages` of them, marks those over `max_image_bytes` as too large, and keeps at most `max_images`. `fetch_images` downloads them concurrently (15 s each, at most `max_image_bytes + 1` bytes read) and checks the magic bytes, which also decide the MIME type. Failures become `[image: … (omitted: download failed)]` and never fail the turn. Images that are not selected stay `[image: …]`.
- **`gateway.rs`** — `presence` turns the activity text into an online custom status that `main.rs` sets on the shard config, so it is sent when identifying. Runs one shard with `EventTypeFlags::all()`, feeds every event to the `DefaultInMemoryCache` (channels only), and stores the bot user id on Ready. A `None` from the stream means the shard closed fatally and the process exits with an error; a user-initiated close ends the loop on the following `GatewayClose`. Each `MessageCreate` is handled in its own task: `is_candidate` (not a bot / system user, allowed guild, `Regular` / `Reply`) and then the pure `decide` on the channel (cache, else `GET /channels/{id}`) and the registry entry. Plain channel + mention → create a thread named by `thread_name` and answer there; locked thread → ignore; reply to someone other than the bot without a mention → ignore (replies whose target is gone still answer); thread owned by the bot or registered → answer; mention → register and answer; unchecked thread → look for the bot in its history once and record the result.
- **`thread.rs`** — `thread_name` (`"{display_name} · {YYYY-MM-DD HH:mm}"` in `thread.timezone`, ≤ 100 chars), `ConversationRegistry` (thread → is a conversation; rebuilt from history after a restart), and `TurnScheduler` (one turn per thread; triggers during a turn coalesce into one more turn).
- **`orchestrator.rs`** — `AppState` and `schedule_turns`. A turn holds a `limits.max_concurrent` semaphore permit, keeps the typing indicator alive, re-fetches the history and the starter message, selects and downloads images, calls `build_context` and the LLM, and posts the answer or an error embed. History comes from REST, which has no `member`, so speaker labels use global name → username. The first turn in a new thread does not reply to the starter message because it lives in the parent channel.
- **`reply.rs`** — `split_message` (≤ 2000 chars, at a paragraph break in the second half of the part, then a line break, then a space, then anywhere; open code blocks are closed and reopened), `answer_parts` (length note, then a `-#` small-text footer: a `🔍 N searches · 🔧 N tool calls` line and up to 5 cited http(s) sources as `<url>` links so Discord does not embed them, all within the `response.max_parts` cap with a note), `error_embed` per `LlmError` variant, and posting with empty `AllowedMentions` so model output never pings.

**Deployment:** `Dockerfile` (repository root, referenced by `release.yaml`) builds with cargo-chef and runs on `gcr.io/distroless/cc-debian12`; TLS uses the system CA bundle shipped there, and time zones come from jiff's bundled tzdb (`tzdb-bundle-always`). `config/` and `.env` are excluded from the build context and mounted at runtime (`compose.yaml`). release-please builds `linux/amd64` and `linux/arm64` on native runners.

**Bot setup:** enable the Message Content privileged intent in the developer portal and grant View Channels, Send Messages, Send Messages in Threads, Create Public Threads, and Read Message History.

## Patterns & Conventions

- **Error handling:** `thiserror::Error` for domain enums (`PythiaConfigError`, `LlmError`, `ContextError`), `anyhow::Result` for propagation up to `main`. Errors inside a turn are logged and, for LLM failures, shown as an embed; they never stop the gateway loop.
- **Statics:** `OnceLock` for config and the bot user id.
- **Secrets:** `EnvConfig` does not derive `Debug`; `PythiaConfig` holds no secrets and is logged in full at `debug`.
- **Lint enforcement:** `#![deny(clippy::all)]` in `main.rs`.
- **Coverage exclusions:** functions that only run against the live gateway or HTTP APIs carry `#[cfg_attr(coverage_nightly, coverage(off))]` so their module's pure logic is still measured; `main.rs` and `orchestrator.rs` are ignored wholesale in `codecov.yml`. Add the attribute to new network-only functions.
- **Logging:** `tracing` macros. Compact or JSON format based on `[log] format`.
- **Testing:** Unit tests in `#[cfg(test)]` modules within each source file. No integration tests. Test names state the guaranteed behavior.
- **Comments:** Only "why not" comments, in English.
- **Commits:** Conventional commits (`feat:`, `fix:`, `chore:`, `ci:`, `refactor:`, `docs:`). Release-please automates versioning and CHANGELOG.

## Environment Variables

- `DISCORD_API_TOKEN` (required) — Bot authentication token
- `OPENROUTER_API_KEY` (required) — OpenRouter API key
- `CONFIG_FILE_PATH` (required) — Path to `config.toml`
- `RUST_LOG` (optional) — Log level filter. Takes precedence over `[log] level` (default `pythia=info`) when set.

Empty values for the required variables are rejected at startup.
