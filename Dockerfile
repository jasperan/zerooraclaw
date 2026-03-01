# syntax=docker/dockerfile:1.7

# ── Stage 1: Build ────────────────────────────────────────────
FROM rust:1.93-slim@sha256:9663b80a1621253d30b146454f903de48f0af925c967be48c84745537cd35d8b AS builder

WORKDIR /app

# Install build dependencies (including Oracle Instant Client for oracle crate)
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y \
        pkg-config \
        libaio1 wget unzip \
    && wget -q https://download.oracle.com/otn_software/linux/instantclient/2350000/instantclient-basiclite-linux.x64-23.5.0.0.0dbru.zip \
    && wget -q https://download.oracle.com/otn_software/linux/instantclient/2350000/instantclient-sdk-linux.x64-23.5.0.0.0dbru.zip \
    && unzip instantclient-basiclite-linux.x64-23.5.0.0.0dbru.zip -d /opt/oracle \
    && unzip instantclient-sdk-linux.x64-23.5.0.0.0dbru.zip -d /opt/oracle \
    && rm -f instantclient-*.zip \
    && echo "/opt/oracle/instantclient_23_5" > /etc/ld.so.conf.d/oracle.conf \
    && ldconfig \
    && rm -rf /var/lib/apt/lists/*

ENV LD_LIBRARY_PATH=/opt/oracle/instantclient_23_5:$LD_LIBRARY_PATH
ENV OCI_LIB_DIR=/opt/oracle/instantclient_23_5
ENV OCI_INC_DIR=/opt/oracle/instantclient_23_5/sdk/include

# 1. Copy manifests to cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY crates/robot-kit/Cargo.toml crates/robot-kit/Cargo.toml
# Create dummy targets declared in Cargo.toml so manifest parsing succeeds.
RUN mkdir -p src benches crates/robot-kit/src \
    && echo "fn main() {}" > src/main.rs \
    && echo "fn main() {}" > benches/agent_benchmarks.rs \
    && echo "pub fn placeholder() {}" > crates/robot-kit/src/lib.rs
RUN --mount=type=cache,id=zeroclaw-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=zeroclaw-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=zeroclaw-target,target=/app/target,sharing=locked \
    cargo build --release --locked
RUN rm -rf src benches crates/robot-kit/src

# 2. Copy only build-relevant source paths (avoid cache-busting on docs/tests/scripts)
COPY src/ src/
COPY benches/ benches/
COPY crates/ crates/
COPY firmware/ firmware/
COPY web/ web/
# Keep release builds resilient when frontend dist assets are not prebuilt in Git.
RUN mkdir -p web/dist && \
    if [ ! -f web/dist/index.html ]; then \
      printf '%s\n' \
        '<!doctype html>' \
        '<html lang="en">' \
        '  <head>' \
        '    <meta charset="utf-8" />' \
        '    <meta name="viewport" content="width=device-width,initial-scale=1" />' \
        '    <title>ZeroOraClaw Dashboard</title>' \
        '  </head>' \
        '  <body>' \
        '    <h1>ZeroOraClaw Dashboard Unavailable</h1>' \
        '    <p>Frontend assets are not bundled in this build. Build the web UI to populate <code>web/dist</code>.</p>' \
        '  </body>' \
        '</html>' > web/dist/index.html; \
    fi
RUN --mount=type=cache,id=zeroclaw-cargo-registry,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=zeroclaw-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=zeroclaw-target,target=/app/target,sharing=locked \
    cargo build --release --locked && \
    cp target/release/zerooraclaw /app/zerooraclaw && \
    strip /app/zerooraclaw

# Prepare runtime directory structure and config derived from canonical config.example.toml
COPY config/config.example.toml /tmp/config.example.toml
RUN mkdir -p /zerooraclaw-data/.zerooraclaw /zerooraclaw-data/workspace && \
    { printf '%s\n' \
        'workspace_dir = "/zerooraclaw-data/workspace"' \
        'config_path = "/zerooraclaw-data/.zerooraclaw/config.toml"' \
        'api_key = ""' \
        ''; \
      cat /tmp/config.example.toml; \
    } > /zerooraclaw-data/.zerooraclaw/config.toml && \
    sed -i \
        -e '/^\[oracle\]/,/^\[/{s/^host = "localhost"/host = "oracle-db"/}' \
        -e '/^\[gateway\]/,/^$/{s/^host = "127\.0\.0\.1"/host = "[::]"/}' \
        -e 's/^# allow_public_bind = true.*/allow_public_bind = true/' \
        -e '/^# host = "\[::\]"/d' \
        /zerooraclaw-data/.zerooraclaw/config.toml && \
    rm /tmp/config.example.toml && \
    chown -R 65534:65534 /zerooraclaw-data

# ── Stage 2: Development Runtime (Debian) ────────────────────
FROM debian:trixie-slim@sha256:f6e2cfac5cf956ea044b4bd75e6397b4372ad88fe00908045e9a0d21712ae3ba AS dev

# Install essential runtime dependencies + Oracle Instant Client
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libaio1 wget unzip \
    && wget -q https://download.oracle.com/otn_software/linux/instantclient/2350000/instantclient-basiclite-linux.x64-23.5.0.0.0dbru.zip \
    && unzip instantclient-basiclite-linux.x64-23.5.0.0.0dbru.zip -d /opt/oracle \
    && rm instantclient-basiclite-linux.x64-23.5.0.0.0dbru.zip \
    && echo "/opt/oracle/instantclient_23_5" > /etc/ld.so.conf.d/oracle.conf \
    && ldconfig \
    && apt-get remove -y wget unzip && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/*

ENV LD_LIBRARY_PATH=/opt/oracle/instantclient_23_5:$LD_LIBRARY_PATH

COPY --from=builder /zerooraclaw-data /zerooraclaw-data
COPY --from=builder /app/zerooraclaw /usr/local/bin/zerooraclaw

# Overwrite minimal config with DEV template (Ollama defaults)
COPY dev/config.template.toml /zerooraclaw-data/.zerooraclaw/config.toml
RUN chown 65534:65534 /zerooraclaw-data/.zerooraclaw/config.toml

# Environment setup
ENV ZEROCLAW_WORKSPACE=/zerooraclaw-data/workspace
ENV HOME=/zerooraclaw-data
ENV PROVIDER="ollama"
ENV ZEROCLAW_MODEL="qwen3:latest"
ENV ZEROCLAW_GATEWAY_PORT=42617

WORKDIR /zerooraclaw-data
USER 65534:65534
EXPOSE 42617
ENTRYPOINT ["zerooraclaw"]
CMD ["gateway"]

# ── Stage 3: Production Runtime (Debian slim, Oracle IC needed) ──
FROM debian:trixie-slim@sha256:f6e2cfac5cf956ea044b4bd75e6397b4372ad88fe00908045e9a0d21712ae3ba AS release

# Install Oracle Instant Client for oracle crate (cannot use distroless)
RUN apt-get update && apt-get install -y --no-install-recommends \
    libaio1 wget unzip ca-certificates && \
    wget -q https://download.oracle.com/otn_software/linux/instantclient/2350000/instantclient-basiclite-linux.x64-23.5.0.0.0dbru.zip && \
    unzip instantclient-basiclite-linux.x64-23.5.0.0.0dbru.zip -d /opt/oracle && \
    rm instantclient-basiclite-linux.x64-23.5.0.0.0dbru.zip && \
    echo "/opt/oracle/instantclient_23_5" > /etc/ld.so.conf.d/oracle.conf && \
    ldconfig && \
    apt-get remove -y wget unzip && apt-get autoremove -y && \
    rm -rf /var/lib/apt/lists/*

ENV LD_LIBRARY_PATH=/opt/oracle/instantclient_23_5:$LD_LIBRARY_PATH

COPY --from=builder /app/zerooraclaw /usr/local/bin/zerooraclaw
COPY --from=builder /zerooraclaw-data /zerooraclaw-data

# Environment setup
ENV ZEROCLAW_WORKSPACE=/zerooraclaw-data/workspace
ENV HOME=/zerooraclaw-data
ENV ZEROCLAW_GATEWAY_PORT=42617

WORKDIR /zerooraclaw-data
USER 65534:65534
EXPOSE 42617
ENTRYPOINT ["zerooraclaw"]
CMD ["gateway"]
