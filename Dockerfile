# syntax=docker/dockerfile:1.7

FROM rust:bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libcurl4-openssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml ./
COPY src ./src

RUN cargo build --release --bin vid

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libcurl4 \
        nodejs \
        python3 \
        python3-pip \
        ffmpeg \
    && ln -sf /usr/bin/nodejs /usr/local/bin/node \
    && python3 -m pip install --no-cache-dir --break-system-packages --upgrade yt-dlp \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/vid /usr/local/bin/vid

WORKDIR /work

ENTRYPOINT ["/bin/sh", "-c", "/usr/local/bin/vid > /work/log.txt 2>&1"]