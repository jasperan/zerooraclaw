# CLAUDE.md — zerooraclaw

> Cross-tool instructions live in [`AGENTS.md`](./AGENTS.md). This file adds Claude Code-specific context on top.

## What This Is

zerooraclaw is an Oracle AI Database-powered fork of zeroclaw — a Rust autonomous agent runtime. The Oracle layer replaces zeroclaw's default storage/memory with Oracle Database (FreePDB or ADB), adding vector search, ONNX-based embeddings, and persistent session state. Binary name: `zerooraclaw`. Lib name: `zeroclaw`. Current version: `0.6.5-oracle.1`.

## Commands

```bash
# Check / lint / test (full CI gate)
just ci                        # fmt-check + clippy + cargo test
just test                      # cargo test --locked (all test suites)
just test-lib                  # unit tests only (faster)
just lint                      # clippy -D warnings
just fmt                       # rustfmt
just build                     # release build
just dev -- [args]             # cargo run -- [args] (debug, with ARGS)

# Or directly:
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo build --release --locked
./dev/ci.sh all                # full pre-PR validation
```

Test suites: `component`, `integration`, `system`, `live` (see `tests/`). Run `live` suite only against a real Oracle instance.

## Project Layout

```
src/
  oracle/          # Oracle layer: connection, memory, vector, session, state, schema
  providers/       # Model provider adapters (trait: src/providers/traits.rs)
  channels/        # Telegram, Discord, Slack, etc (trait: src/channels/traits.rs)
  tools/           # Tool execution surface (trait: src/tools/traits.rs)
  memory/          # Markdown/SQLite backends + embeddings (trait: src/memory/traits.rs)
  gateway/         # Webhook/HTTP server (axum)
  security/        # Policy, pairing, secret store
  agent/           # Orchestration loop
  config/          # Schema + config loading
  peripherals/     # STM32, RPi GPIO (trait: src/peripherals/traits.rs)
oci-genai/         # OCI GenAI Python proxy (oci_client.py, proxy.py)
crates/
  robot-kit/       # Robot/peripheral kit
  aardvark-sys/    # Total Phase Aardvark I2C/SPI stub
apps/tauri/        # Desktop app (Tauri)
```

## Oracle Database Integration

The `src/oracle/` module is the core Oracle fork addition. Two connection modes:

- **FreePDB**: `host:port/service` — default for local Docker setup (`docker compose up -d`, port 1523/FREEPDB1)
- **ADB**: Autonomous Database with DSN — wallet-less TLS or mTLS with wallet

ONNX model names used in `VECTOR_EMBEDDING()` are interpolated (not bound), so they must match `[A-Za-z0-9_.]` or the connection manager rejects them at startup.

Oracle config goes in the zerooraclaw config file (not env vars). The `oracle` crate (`version = "0.6"`) is the Rust driver with `chrono` feature enabled.

## Environment Variables

Copy `.env.example` to `.env`. Key vars:

```bash
PROVIDER=openrouter           # or anthropic, openai, etc.
API_KEY=your-key              # generic fallback; provider-specific vars take priority
ZEROCLAW_STORAGE_PROVIDER=sqlite  # default; Oracle config is in the TOML config file
ZEROCLAW_GATEWAY_PORT=3000
```

Provider-specific keys: `OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `DASHSCOPE_API_KEY`, etc. See `.env.example` for the full list.

## Feature Flags

Default features: `observability-prometheus`, `skill-creation`.

Notable optional features:
- `channel-matrix` — Matrix E2EE (pulls matrix-sdk)
- `channel-lark` / `channel-feishu` — Lark/Feishu WebSocket
- `hardware` — USB device enumeration + serial (nusb, tokio-serial)
- `browser-native` — Fantoccini WebDriver backend
- `sandbox-landlock` — Linux Landlock sandbox
- `whatsapp-web` — wa-rs native WhatsApp client
- `voice-wake` — cpal mic input (needs `libasound2-dev` on Linux; excluded from `ci-all`)
- `rag-pdf` — PDF ingestion via pdf-extract
- `plugins-wasm` — extism WASM plugin runtime
- `observability-otel` — OpenTelemetry OTLP trace + metrics
- `ci-all` — all features safe for CI (excludes `voice-wake`)

Build with specific features: `cargo build --features channel-matrix,hardware`

## Build Profiles

- `dev` — incremental, opt-level 1
- `release` — size-optimized (opt-level z, fat LTO, strip, panic=abort, codegen-units=1; slow compile, ~1GB RAM)
- `release-fast` — like release but codegen-units=8 (needs 16GB+ RAM)
- `ci` — thin LTO, 16 codegen units (fast CI)
- `dist` — identical to release (for distribution artifacts)

On low-RAM machines (RPi 3, 1GB): use `codegen-units=1` (already default in release). Avoid `release-fast`.

## Rust Version

Minimum: `1.87` (edition 2024). The workspace includes `apps/tauri` — Tauri builds require Node.js toolchain separately.

## Gotchas

- `VECTOR_EMBEDDING()` model name cannot be a SQL bind parameter — it's interpolated via `format!()`. The `validate_onnx_model_name()` guard in `src/oracle/connection.rs` enforces `[A-Za-z0-9_.]` only. Pass invalid chars and startup fails.
- `probe-rs` feature (`probe`) adds ~50 extra crates. Avoid enabling it unless doing STM32/Nucleo memory read work.
- `voice-wake` (cpal) needs `libasound2-dev` on Linux. It's deliberately excluded from `ci-all`.
- Release builds strip debug symbols and use `panic=abort` — backtraces won't be useful. Use debug builds for debugging.
- `aardvark-sys` (`crates/aardvark-sys`) is a stub when the Total Phase SDK is absent — it won't link against real hardware without the vendor SDK installed.
- The `oracle` crate requires Oracle Instant Client libraries on the host. On a fresh machine, install OIC before `cargo build`.
- Do not use reserved SQL words as Oracle column names (mode, level, comment, value, date) — use quoted identifiers or rename them to avoid ORA-* errors.

## Risk Tiers (from AGENTS.md)

High-risk areas requiring extra care: `src/security/`, `src/runtime/`, `src/gateway/`, `src/tools/`, `.github/workflows/`. When in doubt, classify higher.
