# Pythia

[![CI](https://github.com/m1sk9/Pythia/actions/workflows/ci.yaml/badge.svg)](https://github.com/m1sk9/Pythia/actions/workflows/ci.yaml)
[![Release Pythia](https://github.com/m1sk9/Pythia/actions/workflows/release.yaml/badge.svg)](https://github.com/m1sk9/Pythia/actions/workflows/release.yaml)
[![Apache License 2.0](https://img.shields.io/github/license/m1sk9/Pythia?color=%239944ee)](https://github.com/m1sk9/Pythia/blob/main/LICENSE)
[![codecov](https://codecov.io/gh/m1sk9/Pythia/graph/badge.svg)](https://codecov.io/gh/m1sk9/Pythia)

A Discord bot that bridges your server and LLM APIs.

> [!WARNING]
> **v2 and earlier are deprecated.**
> Pythia was formerly developed as **ichiyoAI** under the [Approvers](https://github.com/approvers) organization. Those releases (including `ghcr.io/approvers/ichiyo_ai`) may still work, but they are no longer under the control of [@m1sk9](https://github.com/m1sk9) and will not receive any fixes. Use them at your own risk.
> See [SECURITY.md](.github/SECURITY.md) for the supported versions.

```sh
# Latest Release
docker pull ghcr.io/m1sk9/pythia:latest

# Minor Release (v3~)
docker pull ghcr.io/m1sk9/pythia:v3

# Specific Release (v3~)
docker pull ghcr.io/m1sk9/pythia:v3.0.0
```

[_API Support: requires Discord API v10_](https://discord.com/developers/docs/reference#api-versioning)

## Features

- **One thread is one conversation.** Mention Pythia in a channel and it opens a thread from your message and answers there. Keep talking in the thread without mentioning it; it reads the thread history every time, so it keeps no database and survives restarts.
- **Joins existing threads.** Mention Pythia in any thread and it answers there too, and keeps answering without further mentions.
- **Stays out of side conversations.** In a thread, a message that replies to another person is left to that person; reply to Pythia's message, post without replying, or mention it to get an answer.
- **One answer at a time per thread.** Messages sent while Pythia is answering are covered together by its next answer.
- **Reads screenshots.** With a model that accepts images, recent PNG / JPEG / WebP / GIF attachments are sent along with the text. Images are downscaled to `attachments.max_image_edge` px on the longest side before they are sent, so large screenshots still fit and cost fewer tokens.
- **Allow-listed servers only.** Pythia answers only in the guilds listed in `discord.allowed_guilds`.
- **Powered by [OpenRouter](https://openrouter.ai/).** Any chat model on OpenRouter can be used.

Answers longer than Discord's 2000-character limit are split into several messages, and model output never pings users or roles. Failures (timeouts, rate limits, provider errors) are shown as an embed with the status, provider message, and request ID.

## Todo

Planned for v3.x:

- [ ] Server-wide knowledge through a server overview and read-only tools ([#309](https://github.com/m1sk9/Pythia/issues/309))
- [x] Web search and web fetch through OpenRouter server tools ([#317](https://github.com/m1sk9/Pythia/issues/317))
- [ ] Write tools with confirmation buttons and audit log reasons ([#310](https://github.com/m1sk9/Pythia/issues/310))
- [ ] Code execution in a sandbox and external tools ([#311](https://github.com/m1sk9/Pythia/issues/311))
- [ ] OpenAI-compatible and Anthropic backends ([#312](https://github.com/m1sk9/Pythia/issues/312))
- [ ] Cost aggregation per guild ([#314](https://github.com/m1sk9/Pythia/issues/314))
- [ ] PDF and text file attachments ([#315](https://github.com/m1sk9/Pythia/issues/315))
- [x] Downscaling large images ([#316](https://github.com/m1sk9/Pythia/issues/316))

## Setup

1. Create an application and a bot in the [Discord Developer Portal](https://discord.com/developers/applications).
2. Under **Bot**, enable the **Message Content Intent** (privileged).
3. Invite the bot with these permissions:
   - View Channels
   - Send Messages
   - Send Messages in Threads
   - Create Public Threads
   - Read Message History
4. Create an [OpenRouter API key](https://openrouter.ai/settings/keys).

## Configuration

Secrets and the path to the configuration file come from environment variables (a `.env` file is read when present):

| variable             | required | description                                  |
| -------------------- | -------- | -------------------------------------------- |
| `DISCORD_API_TOKEN`  | yes      | Bot token                                    |
| `OPENROUTER_API_KEY` | yes      | OpenRouter API key                           |
| `CONFIG_FILE_PATH`   | yes      | Path to `config.toml`                        |
| `RUST_LOG`           | no       | Log filter; overrides `[log] level` when set |

Everything else lives in `config.toml`. Start from [`config/config.toml`](./config/config.toml), which documents every key and its default. At minimum, set:

```toml
[discord]
allowed_guilds = [123456789012345678]

[llm]
model = "<openrouter model id>"
```

## Running with Docker Compose

Docker Compose is the recommended way to run Pythia. If you use an orchestrator such as Kubernetes or Docker Swarm, translate the following into its configuration.

```yaml
services:
  app:
    image: ghcr.io/m1sk9/pythia:v3
    env_file:
      - .env
    environment:
      CONFIG_FILE_PATH: /config/config.toml
    volumes:
      - ./config/config.toml:/config/config.toml:ro
    restart: always
```

Put `DISCORD_API_TOKEN` and `OPENROUTER_API_KEY` in `.env` next to `compose.yaml`, then:

```sh
docker compose up -d
```

The image is built on distroless and is published for `linux/amd64` and `linux/arm64`.

## Limits and safety defaults

- **Empty `discord.allowed_guilds` refuses to start.** Pythia never answers in a server you did not list.
- **`llm.model` is required.** Models differ widely in price, so there is no default.
- **Bounded usage.** At most `limits.max_concurrent` answers are generated at once, each conversation is trimmed to `context.max_chars` characters (newest messages win), and each answer is capped at `llm.max_output_tokens` tokens and `response.max_parts` messages.
- **Images are opt-out.** Set `attachments.images = false` to send only text. Images are also disabled automatically when the model does not accept them.
- **Images are bounded.** Attachments over `attachments.max_download_bytes` are never downloaded, every image is downscaled to `attachments.max_image_edge` px on the longest side, and anything whose base64 is still over `attachments.max_image_bytes` after that is left out with a note.
- **Other bots are ignored**, as are locked threads and system messages.

## Not supported

- Direct messages
- Slash commands
- Image generation
- Streaming answers (an answer is posted once it is complete)

## LICENSE

Published under [Apache License 2.0](./LICENSE).

Pythia was originally developed as ichiyoAI under the [Approvers](https://github.com/approvers) organization and was relicensed from the MIT License to the Apache License 2.0 with the consent of its contributors.

<sub>
    © 2023 - 2026 m1sk9 and contributors
    <br/>
    Pythia is not affiliated with Discord or any LLM API provider.
</sub>
