#!/bin/bash
# ZeroOraClaw OCI Instance Setup Script
# Runs via cloud-init on first boot - fully unattended
set -euo pipefail
exec > >(tee -a /var/log/zerooraclaw-setup.log) 2>&1

echo "=== ZeroOraClaw setup started at $(date) ==="

ORACLE_MODE="${ORACLE_MODE:-freepdb}"
ORACLE_PWD="${ORACLE_PWD:-ZeroOraClaw2026}"
ADB_DSN="${ADB_DSN:-}"
ADB_WALLET_BASE64="${ADB_WALLET_BASE64:-}"

# -- 1. System packages --
echo "--- Installing system packages ---"
dnf install -y oracle-epel-release-el9
dnf install -y docker-engine git gcc gcc-c++ make wget curl unzip python3 \
  oracle-instantclient-release-el9
dnf install -y oracle-instantclient-basic oracle-instantclient-devel
systemctl enable --now docker
usermod -aG docker opc

# -- 2. Install Rust toolchain --
echo "--- Installing Rust toolchain ---"
export RUSTUP_HOME=/opt/rustup
export CARGO_HOME=/opt/cargo
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
export PATH="/opt/cargo/bin:$PATH"
cat > /etc/profile.d/rust.sh <<'ENVEOF'
export RUSTUP_HOME=/opt/rustup
export CARGO_HOME=/opt/cargo
export PATH="/opt/cargo/bin:$PATH"
ENVEOF
rustc --version
cargo --version

# -- 3. Install Ollama --
echo "--- Installing Ollama ---"
curl -fsSL https://ollama.com/install.sh | sh
systemctl enable --now ollama
sleep 5
ollama pull gemma3:270m
echo "Ollama ready with gemma3:270m"

# -- 4. Build ZeroOraClaw --
echo "--- Building ZeroOraClaw ---"
git clone https://github.com/jasperan/zerooraclaw.git /opt/zerooraclaw
cd /opt/zerooraclaw
# Set Oracle Instant Client paths for the oracle crate build
export ORACLE_HOME=/usr/lib/oracle/23/client64
export LD_LIBRARY_PATH="${ORACLE_HOME}/lib:${LD_LIBRARY_PATH:-}"
cargo build --release
cp target/release/zerooraclaw /usr/local/bin/zerooraclaw
chmod +x /usr/local/bin/zerooraclaw
zerooraclaw --version || true

# -- 5. Initialize config --
echo "--- Initializing config ---"
export HOME=/home/opc
CONFIG_DIR="/home/opc/.zerooraclaw"
CONFIG_FILE="${CONFIG_DIR}/config.toml"
mkdir -p "$CONFIG_DIR"
chown opc:opc "$CONFIG_DIR"

# Write base config with Ollama provider
cat > "$CONFIG_FILE" <<'TOMLEOF'
default_provider = "ollama"
default_model = "gemma3:270m"
default_temperature = 0.7

[memory]
backend = "oracle"
auto_save = true
embedding_provider = "oracle-onnx"
embedding_dimensions = 384
vector_weight = 0.7
keyword_weight = 0.3
min_relevance_score = 0.3

[agent]
max_tool_iterations = 10
max_history_messages = 50

[gateway]
port = 42617
host = "[::]"
allow_public_bind = true
TOMLEOF
chown opc:opc "$CONFIG_FILE"

# -- 6. Oracle Database Setup --
echo "--- Setting up Oracle Database (mode: $ORACLE_MODE) ---"

if [ "$ORACLE_MODE" = "freepdb" ]; then
  # Pull and start Oracle DB Free container
  docker pull container-registry.oracle.com/database/free:latest
  docker run -d --name oracle-free \
    -p 1521:1521 \
    -e ORACLE_PWD="$ORACLE_PWD" \
    -e ORACLE_CHARACTERSET=AL32UTF8 \
    -v oracle-data:/opt/oracle/oradata \
    --restart unless-stopped \
    container-registry.oracle.com/database/free:latest

  echo "Waiting for Oracle DB to be ready..."
  TIMEOUT=300
  ELAPSED=0
  while ! docker logs oracle-free 2>&1 | grep -q "DATABASE IS READY"; do
    sleep 10
    ELAPSED=$((ELAPSED + 10))
    echo "  Waiting... ${ELAPSED}s"
    if [ "$ELAPSED" -ge "$TIMEOUT" ]; then
      echo "ERROR: Oracle DB timed out after ${TIMEOUT}s"
      docker logs oracle-free --tail 50
      exit 1
    fi
  done
  echo "Oracle DB is ready"

  # Create zerooraclaw user
  docker exec oracle-free sqlplus -S "sys/${ORACLE_PWD}@localhost:1521/FREEPDB1 as sysdba" <<SQL || true
WHENEVER SQLERROR CONTINUE
CREATE USER zerooraclaw IDENTIFIED BY "${ORACLE_PWD}"
  DEFAULT TABLESPACE users QUOTA UNLIMITED ON users;
GRANT CONNECT, RESOURCE, DB_DEVELOPER_ROLE TO zerooraclaw;
GRANT CREATE MINING MODEL TO zerooraclaw;
EXIT;
SQL

  # Append Oracle config for freepdb mode
  cat >> "$CONFIG_FILE" <<TOMLEOF

[oracle]
mode = "freepdb"
host = "localhost"
port = 1521
service = "FREEPDB1"
user = "zerooraclaw"
password = "${ORACLE_PWD}"
onnx_model = "ALL_MINILM_L12_V2"
agent_id = "default"
max_connections = 5
TOMLEOF

elif [ "$ORACLE_MODE" = "adb" ]; then
  # ADB mode - wallet and DSN provided by Terraform
  if [ -n "$ADB_WALLET_BASE64" ]; then
    WALLET_DIR="/home/opc/.zerooraclaw/wallet"
    mkdir -p "$WALLET_DIR"
    echo "$ADB_WALLET_BASE64" | base64 -d > "$WALLET_DIR/wallet.zip"
    cd "$WALLET_DIR" && unzip -o wallet.zip && cd -
    chown -R opc:opc "$WALLET_DIR"
  fi

  cat >> "$CONFIG_FILE" <<TOMLEOF

[oracle]
mode = "adb"
wallet_path = "${WALLET_DIR:-}"
service = "${ADB_DSN}"
user = "zerooraclaw"
password = "${ORACLE_PWD}"
onnx_model = "ALL_MINILM_L12_V2"
agent_id = "default"
max_connections = 5
TOMLEOF
fi

chown opc:opc "$CONFIG_FILE"

# -- 7. Initialize Oracle schema --
echo "--- Running setup-oracle ---"
sudo -u opc bash -c "export ORACLE_HOME=/usr/lib/oracle/23/client64 && export LD_LIBRARY_PATH=${ORACLE_HOME}/lib && /usr/local/bin/zerooraclaw setup-oracle"

# -- 8. Run onboard --
echo "--- Running onboard ---"
sudo -u opc bash -c "export ORACLE_HOME=/usr/lib/oracle/23/client64 && export LD_LIBRARY_PATH=${ORACLE_HOME}/lib && /usr/local/bin/zerooraclaw onboard" <<< "n" || true

# -- 9. Install and start gateway systemd service --
echo "--- Installing gateway service ---"
cat > /etc/systemd/system/zerooraclaw-gateway.service <<'UNIT'
[Unit]
Description=ZeroOraClaw Gateway
After=network-online.target docker.service ollama.service
Wants=network-online.target

[Service]
Type=simple
User=opc
Environment=HOME=/home/opc
Environment=ORACLE_HOME=/usr/lib/oracle/23/client64
Environment=LD_LIBRARY_PATH=/usr/lib/oracle/23/client64/lib
ExecStart=/usr/local/bin/zerooraclaw gateway
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
systemctl enable --now zerooraclaw-gateway

# -- 10. Done --
echo "=== ZeroOraClaw setup completed at $(date) ==="
touch /var/log/zerooraclaw-setup-complete
