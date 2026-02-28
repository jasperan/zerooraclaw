<p align="center">
  <img src="zerooraclaw.png" alt="ZeroOraClaw" width="200" />
</p>

<h1 align="center">ZeroOraClaw</h1>

<p align="center">
  <strong>Oracle AI Database-powered AI assistant.</strong> Zero overhead. Zero compromise. 100% Rust.
</p>

<p align="center">
  <a href="LICENSE-APACHE"><img src="https://img.shields.io/badge/license-MIT%20OR%20Apache%202.0-blue.svg?style=for-the-badge" alt="License: MIT OR Apache-2.0" /></a>
  <a href="https://github.com/jasperan/zerooraclaw"><img src="https://img.shields.io/badge/GitHub-zerooraclaw-181717?logo=github&style=for-the-badge" alt="GitHub" /></a>
  <img src="https://img.shields.io/badge/backend-Ollama-black?style=for-the-badge" alt="Ollama" />
  <a href="https://docs.oracle.com/en-us/iaas/Content/generative-ai/home.htm"><img src="https://img.shields.io/badge/OCI-GenAI-F80000.svg?style=for-the-badge&logo=oracle&logoColor=white" alt="OCI GenAI" /></a>
</p>

<p align="center">
  <a href="https://cloud.oracle.com/resourcemanager/stacks/create?zipUrl=https://github.com/jasperan/zerooraclaw/raw/main/deploy/oci/orm/zerooraclaw-orm.zip">
    <img src="https://oci-resourcemanager-plugin.plugins.oci.oraclecloud.com/latest/deploy-to-oracle-cloud.svg" alt="Deploy to Oracle Cloud"/>
  </a>
</p>

---

ZeroOraClaw is a fork of [ZeroClaw](https://github.com/jasperan/zerooraclaw) that replaces **ALL** storage backends with **Oracle AI Database** as the exclusive persistence layer. Every byte of memory, session, state, and embedding lives in Oracle.

## Why Oracle AI Database?

- **In-Database ONNX Embeddings**: Generate 384-dim vectors with `VECTOR_EMBEDDING()` -- zero API calls, zero latency
- **AI Vector Search**: Semantic recall via `VECTOR_DISTANCE()` with COSINE similarity
- **ACID Transactions**: No data loss on crash, ever
- **Multi-Agent Isolation**: Each agent gets its own namespace via `agent_id`
- **Enterprise-Grade**: Connection pooling, automatic indexing, audit trails

## Features

- Everything from ZeroClaw: 30+ LLM providers, 18+ chat channels, tools, skills, robotics
- Oracle AI Database as exclusive storage (no SQLite, no PostgreSQL, no files)
- In-database ONNX embeddings (ALL_MINILM_L12_V2)
- 8 persistent tables (ZERO_* prefix) with vector indexes
- `setup-oracle` CLI for one-command database setup
- `oracle-inspect` CLI dashboard for database inspection
- **Default: [Oracle AI Database 26ai Free](https://www.oracle.com/database/free/) container** for local development
- **Optional: [Oracle Autonomous Database](https://www.oracle.com/autonomous-database/)** for managed cloud deployment via the Deploy to Oracle Cloud button

## Quick Start

### Prerequisites

- Rust 1.87+
- [Oracle AI Database 26ai Free](https://www.oracle.com/database/free/) (Docker) -- the default backend
- Oracle Instant Client (for building)

### 1. Build

```bash
cargo build --release
```

### 2. Start Oracle Database

```bash
./scripts/setup-oracle.sh
# Or manually:
docker compose up oracle-db -d
```

### 3. Initialize

```bash
./target/release/zerooraclaw setup-oracle
./target/release/zerooraclaw onboard
```

### 4. Chat

```bash
./target/release/zerooraclaw agent -m "Hello! Remember that I love Rust."
./target/release/zerooraclaw agent -m "What programming language do I like?"
```

### 5. Inspect

```bash
./target/release/zerooraclaw oracle-inspect
./target/release/zerooraclaw oracle-inspect memories --search "programming"
```

## Docker Compose

```bash
# Full stack: Oracle AI Database 26ai Free + ZeroOraClaw
docker compose up -d

# With custom API key
API_KEY=sk-... docker compose up -d

# Oracle AI Database 26ai Free only (for local development)
docker compose up oracle-db -d
```

The Oracle AI Database 26ai Free container takes approximately 2 minutes to initialize on first start. The `zerooraclaw` service will wait for it to become healthy before starting.

## OCI Generative AI (Optional)

ZeroOraClaw can optionally use **OCI Generative AI** as an LLM backend via the `oci-openai` Python library. This is **not required** -- Ollama remains the default and recommended LLM backend.

### Why OCI GenAI?

- **Enterprise models** -- Access xAI Grok, Meta Llama, Cohere, and other models through OCI
- **OCI-native auth** -- Uses your existing `~/.oci/config` profile (no separate API keys)
- **Same region as your database** -- Run inference and storage in the same OCI region

### Setup

1. **Install the OCI GenAI proxy:**
   ```bash
   cd oci-genai
   pip install -r requirements.txt
   ```

2. **Configure OCI credentials** (`~/.oci/config`):
   ```ini
   [DEFAULT]
   user=ocid1.user.oc1..aaaaaaaaexample
   fingerprint=aa:bb:cc:dd:ee:ff:00:11:22:33:44:55:66:77:88:99
   tenancy=ocid1.tenancy.oc1..aaaaaaaaexample
   region=us-chicago-1
   key_file=~/.oci/oci_api_key.pem
   ```

3. **Set environment variables:**
   ```bash
   export OCI_PROFILE=DEFAULT
   export OCI_REGION=us-chicago-1
   export OCI_COMPARTMENT_ID=ocid1.compartment.oc1..your-compartment-ocid
   ```

4. **Start the OCI GenAI proxy:**
   ```bash
   cd oci-genai
   python proxy.py
   # Proxy runs at http://localhost:9999/v1
   ```

5. **Configure ZeroOraClaw** (`~/.zerooraclaw/config.toml`):
   ```toml
   [provider]
   name = "openai"
   api_base = "http://localhost:9999/v1"
   api_key = "oci-genai"
   model = "meta.llama-3.3-70b-instruct"
   ```

   Or via environment variables:
   ```bash
   PROVIDER=openai API_KEY=oci-genai ./zerooraclaw agent -m "Hello"
   ```

See [`oci-genai/README.md`](oci-genai/README.md) for full documentation.

## Oracle Schema

| Table | Purpose | Key Feature |
|---|---|---|
| ZERO_META | Schema version | Single row per agent |
| ZERO_MEMORIES | Long-term memories | VECTOR(384) + COSINE index |
| ZERO_DAILY_NOTES | Daily journal | VECTOR(384) + COSINE index |
| ZERO_SESSIONS | Chat history | JSON CLOB per channel |
| ZERO_TRANSCRIPTS | Full audit log | IDENTITY sequence PK |
| ZERO_STATE | Agent K-V state | Composite PK |
| ZERO_CONFIG | Config snapshots | JSON CLOB |
| ZERO_PROMPTS | System prompts | Seeded from workspace |

## Configuration

```toml
# ~/.zerooraclaw/config.toml

[oracle]
mode = "freepdb"           # "freepdb" for 26ai Free container (default) | "adb" for Autonomous DB (cloud)
host = "localhost"
port = 1521
service = "FREEPDB1"
user = "zerooraclaw"
password = "ZeroOraClaw2026"
onnx_model = "ALL_MINILM_L12_V2"
agent_id = "default"
```

See `config/config.example.toml` for the complete reference with all available options.

### Environment Variables

Oracle connection settings can also be configured via environment variables:

| Variable | Description | Default |
|---|---|---|
| `ZEROORACLAW_ORACLE_HOST` | Database hostname | `localhost` |
| `ZEROORACLAW_ORACLE_PORT` | Listener port | `1521` |
| `ZEROORACLAW_ORACLE_SERVICE` | Service name | `FREEPDB1` |
| `ZEROORACLAW_ORACLE_USER` | Database user | `zerooraclaw` |
| `ZEROORACLAW_ORACLE_PASSWORD` | Database password | -- |
| `API_KEY` | LLM provider API key | -- |
| `PROVIDER` | LLM provider name | `ollama` |

## Architecture

```
zerooraclaw
  src/
    oracle/            # Oracle AI Database integration
      connection.rs    # Connection pooling and lifecycle
      schema.rs        # 8 ZERO_* table DDL + ONNX model loading
      embedding.rs     # In-database VECTOR_EMBEDDING() service
      memory.rs        # Memory trait backed by Oracle + vector search
      session.rs       # Chat session persistence (JSON CLOB)
      state.rs         # Agent key-value state store
      config_store.rs  # Config snapshot persistence
      prompt.rs        # System prompt persistence
      vector.rs        # Vector distance helpers
      mod.rs           # Module exports
    memory/            # Memory trait definitions
    agent/             # Agent runtime loop
    cli/               # CLI commands (setup-oracle, oracle-inspect)
    ...
```

## Deploy to Oracle Cloud (One-Click)

[![Deploy to Oracle Cloud](https://oci-resourcemanager-plugin.plugins.oci.oraclecloud.com/latest/deploy-to-oracle-cloud.svg)](https://cloud.oracle.com/resourcemanager/stacks/create?zipUrl=https://github.com/jasperan/zerooraclaw/raw/main/deploy/oci/orm/zerooraclaw-orm.zip)

This deploys a fully configured ZeroOraClaw instance on OCI with:

- **Oracle Linux 9** compute instance (ARM A1.Flex -- Always Free eligible)
- **Ollama** with gemma3:270m model pre-installed
- **Oracle AI Database 26ai Free** container by default (or optional Autonomous AI Database when toggled)
- **ZeroOraClaw** built from source with Oracle schema initialized
- **Gateway** running as a systemd service on port 42617

After deployment, check the Terraform outputs for your instance IP and run:

```bash
# Watch setup progress (~10 min for Rust build + Oracle init)
ssh opc@<instance-ip> -t 'tail -f /var/log/zerooraclaw-setup.log'

# Start chatting
ssh opc@<instance-ip> -t zerooraclaw agent

# Check gateway health
curl http://<instance-ip>:42617/health
```

## Sister Projects

- [PicoOraClaw](https://github.com/jasperan/picooraclaw) -- Go-based, same Oracle pattern
- [OracLaw](https://github.com/jasperan/oraclaw) -- TypeScript + Python sidecar

## Credits

- [ZeroClaw](https://github.com/jasperan/zerooraclaw) -- the Rust AI agent runtime this project is forked from
- [Oracle AI Database](https://www.oracle.com/database/) -- the exclusive storage backbone

## License

MIT OR Apache-2.0

---

<div align="center">

[![GitHub](https://img.shields.io/badge/GitHub-jasperan-181717?style=for-the-badge&logo=github&logoColor=white)](https://github.com/jasperan)&nbsp;
[![LinkedIn](https://img.shields.io/badge/LinkedIn-jasperan-0077B5?style=for-the-badge&logo=linkedin&logoColor=white)](https://www.linkedin.com/in/jasperan/)&nbsp;
[![Oracle](https://img.shields.io/badge/Oracle_AI_Database-26ai_Free-F80000?style=for-the-badge&logo=oracle&logoColor=white)](https://www.oracle.com/database/free/)

</div>
