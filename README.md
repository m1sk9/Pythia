# Pythia

[![CI](https://github.com/m1sk9/Pythia/actions/workflows/ci.yaml/badge.svg)](https://github.com/m1sk9/Pythia/actions/workflows/ci.yaml)
[![Release Pythia](https://github.com/m1sk9/Pythia/actions/workflows/release.yaml/badge.svg)](https://github.com/m1sk9/Pythia/actions/workflows/release.yaml)
[![Apache License 2.0](https://img.shields.io/github/license/m1sk9/Pythia?color=%239944ee)](https://github.com/m1sk9/Pythia/blob/main/LICENSE)

A Discord bot that bridges your server and LLM APIs.

> [!CAUTION]
> **Pythia is under heavy rewrite and is not usable until v3.0.0 is released.**
> The code on `main` is broken and incomplete. Do not deploy it, and do not expect any of the images below to exist until v3.0.0 is published.

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

## Installation

You can install Pythia using Docker. The following command will pull the latest version of Pythia.

```shell
docker pull ghcr.io/m1sk9/pythia:v3
```

Tested on macOS and Linux (major distributions) as recommended environment.

### Using Docker Compose

It is recommended to use Docker Compose when setting up Pythia. Direct startup using Docker images or binary files is also possible but not recommended.

If you are using orchestration tools such as k8s or Docker Swarm, please configure them according to their respective configuration files.

```yaml
services:
  app:
    image: ghcr.io/m1sk9/pythia:v3
    env_file:
      - .env
    restart: always
```

## LICENSE

Published under [Apache License 2.0](./LICENSE).

Pythia was originally developed as ichiyoAI under the [Approvers](https://github.com/approvers) organization († 限界開発鯖 †) and was relicensed from the MIT License to the Apache License 2.0 with the consent of its contributors.

<sub>
    © 2023 - 2026 m1sk9 and contributors
    <br/>
    Pythia is not affiliated with Discord or any LLM API provider.
</sub>
