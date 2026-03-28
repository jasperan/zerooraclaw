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

# -- 3. Install Ollama (pull model in background) --
echo "--- Installing Ollama ---"
curl -fsSL https://ollama.com/install.sh | sh
systemctl enable --now ollama

# Pull model in background while cargo builds
(
  echo "--- Waiting for Ollama to be ready ---"
  for i in $(seq 1 30); do
    if ollama list >/dev/null 2>&1; then break; fi
    sleep 2
  done
  ollama pull gemma3:270m
  echo "Ollama ready with gemma3:270m"
) &
OLLAMA_PID=$!

# -- 4. Start Oracle DB early (runs in parallel with cargo build) --
if [ "$ORACLE_MODE" = "freepdb" ]; then
  echo "--- Pulling Oracle DB container (background) ---"
  docker pull container-registry.oracle.com/database/free:latest
  docker run -d --name oracle-free \
    -p 1521:1521 \
    -e ORACLE_PWD="$ORACLE_PWD" \
    -e ORACLE_CHARACTERSET=AL32UTF8 \
    -v oracle-data:/opt/oracle/oradata \
    --restart unless-stopped \
    container-registry.oracle.com/database/free:latest
  echo "Oracle DB container started, will wait for readiness after cargo build"
fi

# -- 5. Build ZeroOraClaw (CPU-intensive, runs while DB starts) --
echo "--- Building ZeroOraClaw ---"
git clone --depth 1 https://github.com/jasperan/zerooraclaw.git /opt/zerooraclaw
cd /opt/zerooraclaw
# Set Oracle Instant Client paths for the oracle crate build
export ORACLE_HOME=/usr/lib/oracle/23/client64
export LD_LIBRARY_PATH="${ORACLE_HOME}/lib:${LD_LIBRARY_PATH:-}"
cargo build --release
cp target/release/zerooraclaw /usr/local/bin/zerooraclaw
chmod +x /usr/local/bin/zerooraclaw
zerooraclaw --version || true

# -- 6. Initialize config (derived from canonical config.example.toml) --
echo "--- Initializing config ---"
export HOME=/home/opc
CONFIG_DIR="/home/opc/.zerooraclaw"
CONFIG_FILE="${CONFIG_DIR}/config.toml"
mkdir -p "$CONFIG_DIR"
chown opc:opc "$CONFIG_DIR"

# Copy canonical config from cloned repo and customize for cloud deployment
cp /opt/zerooraclaw/config/config.example.toml "$CONFIG_FILE"
sed -i \
    -e 's/^default_model = .*/default_model = "gemma3:270m"/' \
    -e '/^\[gateway\]/,/^\[/{s/^host = "127\.0\.0\.1"/host = "[::]"/}' \
    -e 's/^# allow_public_bind = true.*/allow_public_bind = true/' \
    -e '/^# host = "\[::\]"/d' \
    "$CONFIG_FILE"
# Remove the example [oracle] section — will be appended based on deployment mode
awk '/^# ── Oracle/{skip=1} /^# ── Memory/{skip=0} !skip' "$CONFIG_FILE" > "${CONFIG_FILE}.tmp"
mv "${CONFIG_FILE}.tmp" "$CONFIG_FILE"
chown opc:opc "$CONFIG_FILE"

# -- 7. Oracle Database Setup --
echo "--- Setting up Oracle Database (mode: $ORACLE_MODE) ---"

if [ "$ORACLE_MODE" = "freepdb" ]; then
  # Wait for Oracle DB to be ready (container was started in step 4)
  echo "Waiting for Oracle DB to be ready..."
  TIMEOUT=300
  ELAPSED=0
  while ! docker exec oracle-free sqlplus -S /nolog <<< "CONNECT sys/${ORACLE_PWD}@localhost:1521/FREEPDB1 AS SYSDBA
SELECT 1 FROM DUAL;
EXIT;" >/dev/null 2>&1; do
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

  # Create zerooraclaw user (idempotent — matches scripts/setup-oracle.sh)
  docker exec oracle-free sqlplus -S "sys/${ORACLE_PWD}@localhost:1521/FREEPDB1 as sysdba" <<SQL || true
-- Create user (handle ORA-01920 if user already exists)
DECLARE
  user_exists EXCEPTION;
  PRAGMA EXCEPTION_INIT(user_exists, -1920);
BEGIN
  EXECUTE IMMEDIATE 'CREATE USER zerooraclaw IDENTIFIED BY "${ORACLE_PWD}"';
EXCEPTION
  WHEN user_exists THEN
    EXECUTE IMMEDIATE 'ALTER USER zerooraclaw IDENTIFIED BY "${ORACLE_PWD}"';
END;
/

-- Grant privileges (aligned with scripts/setup-oracle.sh)
GRANT CONNECT, RESOURCE, CREATE SESSION TO zerooraclaw;
GRANT CREATE TABLE, CREATE SEQUENCE TO zerooraclaw;
GRANT CREATE MINING MODEL TO zerooraclaw;
GRANT DB_DEVELOPER_ROLE TO zerooraclaw;
GRANT UNLIMITED TABLESPACE TO zerooraclaw;

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
  # Autonomous AI Database mode (optional cloud backend) - wallet and DSN provided by Terraform
  if [ -n "$ADB_WALLET_BASE64" ]; then
    WALLET_DIR="/home/opc/.zerooraclaw/wallet"
    mkdir -p "$WALLET_DIR"
    echo "$ADB_WALLET_BASE64" | base64 -d > "$WALLET_DIR/wallet.zip"
    unzip -o "$WALLET_DIR/wallet.zip" -d "$WALLET_DIR"
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

# -- 8. Initialize Oracle schema --
echo "--- Running setup-oracle ---"
sudo -u opc bash -c 'export ORACLE_HOME=/usr/lib/oracle/23/client64 && export LD_LIBRARY_PATH=$ORACLE_HOME/lib && /usr/local/bin/zerooraclaw setup-oracle'

# -- 9. Run onboard --
echo "--- Running onboard ---"
sudo -u opc bash -c 'export ORACLE_HOME=/usr/lib/oracle/23/client64 && export LD_LIBRARY_PATH=$ORACLE_HOME/lib && /usr/local/bin/zerooraclaw onboard' <<< "n" || true

# Wait for background Ollama model pull to complete
echo "--- Waiting for Ollama model pull to finish ---"
wait $OLLAMA_PID || true

# -- 10. Install and start gateway systemd service --
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

# -- 11. Done --
echo "=== ZeroOraClaw setup completed at $(date) ==="
touch /var/log/zerooraclaw-setup-complete
