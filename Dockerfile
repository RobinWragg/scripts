# syntax=docker/dockerfile:1.7

FROM rust:bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libcurl4-openssl-dev \
        libgtk-3-dev \
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
        curl \
        unzip \
        libcurl4 \
        nodejs \
        ffmpeg \
    && curl -fsSL https://deno.land/install.sh | DENO_INSTALL=/usr/local sh \
    && /usr/local/bin/deno --version >/dev/null \
    && rm -rf /var/lib/apt/lists/*

ENV VID_YTDLP_JS_RUNTIMES="deno:/usr/local/bin/deno,node:/usr/bin/node,node:/usr/bin/nodejs"

COPY --from=builder /app/target/release/vid /usr/local/bin/vid
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint
RUN chmod 0755 /usr/local/bin/docker-entrypoint

WORKDIR /work

ENTRYPOINT ["/usr/local/bin/docker-entrypoint"]