FROM rust:1.97.1-bookworm AS builder

WORKDIR /root/app
COPY --chown=root:root . .

RUN cargo build --release --bin pythia

FROM debian:bookworm-slim AS runner

COPY --from=builder --chown=root:root /root/app/target/release/pythia /usr/local/bin/pythia

RUN apt-get update && apt-get install -y libssl-dev ca-certificates

RUN useradd --create-home --user-group pythia
USER pythia
WORKDIR /home/pythia

LABEL org.opencontainers.image.source=https://github.com/m1sk9/Pythia
ENTRYPOINT [ "pythia" ]
