#!/usr/bin/env bash
set -euo pipefail

echo "=== ZeroOraClaw Oracle Setup ==="
echo

ORACLE_PASSWORD="${ZEROORACLAW_ORACLE_PASSWORD:-ZeroOraClaw2026}"
CONTAINER_NAME="zerooraclaw-oracle"

if [ ! -f docker-compose.yml ]; then
  echo "ERROR: run this from the repository root"
  exit 1
fi

echo "Starting Oracle AI Database Free container..."
docker compose up oracle-db -d

echo
echo "Waiting for Oracle to become healthy (first boot can take ~2 minutes)..."
MAX_WAIT=300
ELAPSED=0
STATUS="starting"
while [ "$ELAPSED" -lt "$MAX_WAIT" ]; do
  STATUS=$(docker inspect --format='{{if .State.Health}}{{.State.Health.Status}}{{else}}starting{{end}}' "$CONTAINER_NAME" 2>/dev/null || echo "starting")
  if [ "$STATUS" = "healthy" ]; then
    echo "Oracle is healthy."
    break
  fi
  sleep 5
  ELAPSED=$((ELAPSED + 5))
  echo "  Waiting... (${ELAPSED}s)"
done

if [ "$STATUS" != "healthy" ]; then
  echo "ERROR: Oracle did not become healthy within ${MAX_WAIT}s"
  echo "Check logs with: docker compose logs oracle-db"
  exit 1
fi

echo
echo "Creating/updating zerooraclaw database user..."
docker exec "$CONTAINER_NAME" sqlplus -S "sys/${ORACLE_PASSWORD}@localhost:1521/FREEPDB1 as sysdba" <<SQL || true
DECLARE
  user_exists EXCEPTION;
  PRAGMA EXCEPTION_INIT(user_exists, -1920);
BEGIN
  EXECUTE IMMEDIATE 'CREATE USER zerooraclaw IDENTIFIED BY "${ORACLE_PASSWORD}"';
EXCEPTION
  WHEN user_exists THEN
    EXECUTE IMMEDIATE 'ALTER USER zerooraclaw IDENTIFIED BY "${ORACLE_PASSWORD}"';
END;
/
GRANT CONNECT, RESOURCE, CREATE SESSION TO zerooraclaw;
GRANT CREATE TABLE, CREATE SEQUENCE TO zerooraclaw;
GRANT CREATE MINING MODEL TO zerooraclaw;
GRANT DB_DEVELOPER_ROLE TO zerooraclaw;
GRANT UNLIMITED TABLESPACE TO zerooraclaw;
EXIT;
SQL

echo
echo "Oracle DB is ready for ZeroOraClaw."
echo "Connection: localhost:1521/FREEPDB1"
echo "User: zerooraclaw"
echo
if [ ! -f .env ]; then
  echo "Tip: copy .env.example to .env before running the app."
fi
echo "Next steps:"
echo "  cargo build --release"
echo "  ./target/release/zeroclaw setup-oracle"
echo "  ./target/release/zeroclaw onboard"
