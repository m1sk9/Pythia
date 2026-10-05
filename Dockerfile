FROM lukemathwalker/cargo-chef:latest-rust-1.99.0-bookworm AS chef
WORKDIR /root/app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS cook
COPY --from=planner /root/app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

FROM cook AS builder
COPY . .
RUN cargo build --release --bin pythia

FROM gcr.io/distroless/cc-debian12 AS runner

COPY --from=builder --chown=root:root /root/app/target/release/pythia /

LABEL org.opencontainers.image.source=https://github.com/m1sk9/Pythia

# Secrets and CONFIG_FILE_PATH come from the environment (see compose.yaml).
# The log level follows `[log] level` in config.toml; RUST_LOG overrides it.

CMD ["./pythia"]
