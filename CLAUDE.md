# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Pythia is a Discord bot that bridges a server and LLM APIs: one thread is one conversation, and the bot keeps no state beyond what Discord holds. v3 is a rewrite on twilight and Tokio (Rust edition 2024, MSRV 1.89). As of Phase 1 the bot loads its configuration, connects to the Discord gateway, logs the Ready event, and shuts down cleanly; it does not respond to messages yet.

## Common Commands

```bash
cargo build                                    # Debug build
cargo test --verbose                           # Run all tests
cargo fmt --all -- --check                     # Check formatting
cargo clippy --all-targets --all-features      # Lint
CONFIG_FILE_PATH=config/config.toml cargo run  # Run (tokens from the environment or .env)
```

## Architecture

The code is split into three layers: **on-event** decides whether a gateway event should start a turn, the **orchestrator** turns a thread into an LLM request and back, and **interact-to** posts the result to Discord.

**Entry flow:** `main.rs` → loads env + TOML config → initializes logging → builds the twilight-http client and shard → `gateway.rs` event loop → graceful shutdown on Ctrl-C / SIGTERM

| module | layer | role | status |
|---|---|---|---|
| `main.rs` | wiring | config → logging → clients → gateway loop → shutdown | done |
| `config.rs` | — | env + TOML config | done |
| `gateway.rs` | on-event | shard loop, cache, trigger decision | shard loop and cache (Phase 1) |
| `thread.rs` | on-event | thread naming, conversation registry, per-thread turn state | planned |
| `context.rs` | orchestrator | Discord messages → chat messages (pure) | planned |
| `attachments.rs` | orchestrator | image download / validation / base64 | planned |
| `llm/mod.rs`, `llm/openrouter.rs` | orchestrator | chat-completions shape and the OpenRouter client | planned (#292) |
| `orchestrator.rs` | orchestrator | one conversation turn | planned |
| `reply.rs` | interact-to | message splitting, error embeds, posting | planned |

**Key modules:**

- **`config.rs`** — `EnvConfig` (secrets, via `envy`) and `PythiaConfig` (TOML, via `CONFIG_FILE_PATH`), both stored in `OnceLock`s. The file is deserialized into private `Raw*` structs and then validated into the public types, so required keys (`llm.model`) are reported by their full path and `system_prompt` / `system_prompt_file` are resolved into a single string. `config/config.toml` is the reference config and is covered by a test.
- **`gateway.rs`** — Runs one shard with `EventTypeFlags::all()`, feeds every event to a `DefaultInMemoryCache` (channels only), and stores the bot user id on Ready. A `None` from the stream means the shard closed fatally and the process exits with an error; a user-initiated close ends the loop on the following `GatewayClose`.

## Patterns & Conventions

- **Error handling:** `thiserror::Error` for domain enums (`PythiaConfigError`), `anyhow::Result` for propagation up to `main`.
- **Statics:** `OnceLock` for config and the bot user id.
- **Secrets:** `EnvConfig` does not derive `Debug`; `PythiaConfig` holds no secrets and is logged in full at `debug`.
- **Lint enforcement:** `#![deny(clippy::all)]` in `main.rs`. Code reserved for later phases carries `#[expect(dead_code, reason = ...)]` so it is flagged once it starts being used.
- **Logging:** `tracing` macros. Compact or JSON format based on `[log] format`.
- **Testing:** Unit tests in `#[cfg(test)]` modules within each source file. No integration tests. Test names state the guaranteed behavior.
- **Comments:** Only "why not" comments, in English.
- **Commits:** Conventional commits (`feat:`, `fix:`, `chore:`, `ci:`, `refactor:`, `docs:`). Release-please automates versioning and CHANGELOG.

## Environment Variables

- `DISCORD_API_TOKEN` (required) — Bot authentication token
- `OPENROUTER_API_KEY` (required) — OpenRouter API key (not used until the OpenRouter client lands)
- `CONFIG_FILE_PATH` (required) — Path to `config.toml`
- `RUST_LOG` (optional) — Log level filter. Takes precedence over `[log] level` (default `pythia=info`) when set.

Empty values for the required variables are rejected at startup.
