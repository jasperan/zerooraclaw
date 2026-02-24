#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────
# ZeroOraClaw — Oracle Database setup script
#
# This script:
#   1. Starts the Oracle Database Free container (or checks if running)
#   2. Waits for the database to become healthy
#   3. Creates the zerooraclaw database user with required grants
#   4. Runs `zerooraclaw setup-oracle` to initialize the schema
#
# Usage:
#   ./scripts/setup-oracle.sh
#   ORACLE_PWD=MyPassword ./scripts/setup-oracle.sh
# ──────────────────────────────────────────────────────────────
set -euo pipefail

ORACLE_PWD="${ORACLE_PWD:-ZeroOraClaw2026}"
ORACLE_CONTAINER="${ORACLE_CONTAINER:-zerooraclaw-oracle}"
ORACLE_PORT="${ORACLE_PORT:-1521}"
ORACLE_SERVICE="${ORACLE_SERVICE:-FREEPDB1}"
ORACLE_USER="${ORACLE_USER:-zerooraclaw}"
MAX_WAIT_SECONDS="${MAX_WAIT_SECONDS:-600}"

info()  { echo "==> $*"; }
warn()  { echo "warning: $*" >&2; }
error() { echo "error: $*" >&2; }

# ── 1. Ensure the Oracle container is running ─────────────────

CONTAINER_CLI="${CONTAINER_CLI:-docker}"
if ! command -v "$CONTAINER_CLI" >/dev/null 2>&1; then
    if command -v podman >/dev/null 2>&1; then
        CONTAINER_CLI="podman"
    else
        error "Neither docker nor podman found. Install one and try again."
        exit 1
    fi
fi

container_running() {
    "$CONTAINER_CLI" inspect -f '{{.State.Running}}' "$ORACLE_CONTAINER" 2>/dev/null | grep -q true
}

container_exists() {
    "$CONTAINER_CLI" inspect "$ORACLE_CONTAINER" >/dev/null 2>&1
}

if container_running; then
    info "Oracle container '$ORACLE_CONTAINER' is already running."
elif container_exists; then
    info "Starting existing Oracle container '$ORACLE_CONTAINER'..."
    "$CONTAINER_CLI" start "$ORACLE_CONTAINER"
else
    info "Starting Oracle Database Free via docker compose..."
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
    ORACLE_PWD="$ORACLE_PWD" "$CONTAINER_CLI" compose -f "$PROJECT_DIR/docker-compose.yml" up oracle-db -d
fi

# ── 2. Wait for the database to become healthy ───────────────

info "Waiting for Oracle Database to become healthy (up to ${MAX_WAIT_SECONDS}s)..."

elapsed=0
interval=10
while [ "$elapsed" -lt "$MAX_WAIT_SECONDS" ]; do
    # Check container health status
    health=$("$CONTAINER_CLI" inspect -f '{{.State.Health.Status}}' "$ORACLE_CONTAINER" 2>/dev/null || echo "unknown")

    if [ "$health" = "healthy" ]; then
        info "Oracle Database is healthy."
        break
    fi

    # Also try a direct connection test
    if "$CONTAINER_CLI" exec "$ORACLE_CONTAINER" bash -c \
        "echo 'SELECT 1 FROM DUAL;' | sqlplus -s sys/\"${ORACLE_PWD}\"@localhost:${ORACLE_PORT}/${ORACLE_SERVICE} as sysdba" \
        >/dev/null 2>&1; then
        info "Oracle Database is responding to queries."
        break
    fi

    printf "  ... still waiting (%ds elapsed, health=%s)\n" "$elapsed" "$health"
    sleep "$interval"
    elapsed=$((elapsed + interval))
done

if [ "$elapsed" -ge "$MAX_WAIT_SECONDS" ]; then
    error "Oracle Database did not become healthy within ${MAX_WAIT_SECONDS} seconds."
    error "Check container logs: $CONTAINER_CLI logs $ORACLE_CONTAINER"
    exit 1
fi

# ── 3. Create the zerooraclaw database user ───────────────────

info "Creating database user '${ORACLE_USER}'..."

"$CONTAINER_CLI" exec "$ORACLE_CONTAINER" bash -c "
sqlplus -s sys/\"${ORACLE_PWD}\"@localhost:${ORACLE_PORT}/${ORACLE_SERVICE} as sysdba <<EOSQL
-- Create user (ignore ORA-01920 if user already exists)
DECLARE
  user_exists EXCEPTION;
  PRAGMA EXCEPTION_INIT(user_exists, -1920);
BEGIN
  EXECUTE IMMEDIATE 'CREATE USER ${ORACLE_USER} IDENTIFIED BY \"${ORACLE_PWD}\"';
  DBMS_OUTPUT.PUT_LINE('User ${ORACLE_USER} created.');
EXCEPTION
  WHEN user_exists THEN
    DBMS_OUTPUT.PUT_LINE('User ${ORACLE_USER} already exists, updating password.');
    EXECUTE IMMEDIATE 'ALTER USER ${ORACLE_USER} IDENTIFIED BY \"${ORACLE_PWD}\"';
END;
/

-- Grant privileges
GRANT CONNECT, RESOURCE, CREATE SESSION TO ${ORACLE_USER};
GRANT CREATE TABLE, CREATE SEQUENCE TO ${ORACLE_USER};
GRANT CREATE MINING MODEL TO ${ORACLE_USER};
GRANT DB_DEVELOPER_ROLE TO ${ORACLE_USER};
GRANT UNLIMITED TABLESPACE TO ${ORACLE_USER};

COMMIT;
EXIT;
EOSQL
"

info "Database user '${ORACLE_USER}' is ready."

# ── 4. Run zerooraclaw setup-oracle ──────────────────────────

ZEROORACLAW_BIN=""
if command -v zerooraclaw >/dev/null 2>&1; then
    ZEROORACLAW_BIN="zerooraclaw"
elif [ -x "./target/release/zerooraclaw" ]; then
    ZEROORACLAW_BIN="./target/release/zerooraclaw"
elif [ -x "./target/debug/zerooraclaw" ]; then
    ZEROORACLAW_BIN="./target/debug/zerooraclaw"
fi

if [ -n "$ZEROORACLAW_BIN" ]; then
    info "Running schema initialization: $ZEROORACLAW_BIN setup-oracle"
    ZEROORACLAW_ORACLE_HOST="${ZEROORACLAW_ORACLE_HOST:-localhost}" \
    ZEROORACLAW_ORACLE_PORT="${ORACLE_PORT}" \
    ZEROORACLAW_ORACLE_SERVICE="${ORACLE_SERVICE}" \
    ZEROORACLAW_ORACLE_USER="${ORACLE_USER}" \
    ZEROORACLAW_ORACLE_PASSWORD="${ORACLE_PWD}" \
        "$ZEROORACLAW_BIN" setup-oracle
else
    warn "zerooraclaw binary not found. Build first with 'cargo build --release'."
    warn "Then run: zerooraclaw setup-oracle"
fi

info "Oracle Database setup complete."
echo ""
echo "  Connection details:"
echo "    Host:     localhost"
echo "    Port:     ${ORACLE_PORT}"
echo "    Service:  ${ORACLE_SERVICE}"
echo "    User:     ${ORACLE_USER}"
echo "    Password: (as configured)"
echo ""
echo "  Next steps:"
echo "    zerooraclaw setup-oracle   # Initialize schema (if not done above)"
echo "    zerooraclaw onboard        # Configure LLM provider"
echo "    zerooraclaw agent -m 'Hello!'  # Start chatting"
