#!/usr/bin/env bash
set -euo pipefail

DB_URI="${DB_URI:-mongodb://mongodb:27017}"
DB_DATABASE="${DB_DATABASE:-credit_exchanger}"
RUN_CREDIT_EXCHANGER_SEED="${RUN_CREDIT_EXCHANGER_SEED:-true}"
BLACKOUT_CONTROLLER_URL="${BLACKOUT_CONTROLLER_URL:-http://BLACKOUT-SERVICE}"
AI_WO_A_CONTROLLER_URL="${AI_WO_A_CONTROLLER_URL:-http://AI-WO-A-SERVICE}"
RUST_LOG="${RUST_LOG:-info}"
CREDIT_EXCHANGER_PROXY_PORT="${CREDIT_EXCHANGER_PROXY_PORT:-18080}"

export DB_URI DB_DATABASE BLACKOUT_CONTROLLER_URL AI_WO_A_CONTROLLER_URL RUST_LOG

for _ in {1..60}; do
  if mongosh "$DB_URI/admin" --quiet --eval 'db.runCommand({ ping: 1 }).ok' | grep -q 1; then
    break
  fi

  sleep 1
done

if ! mongosh "$DB_URI/admin" --quiet --eval 'db.runCommand({ ping: 1 }).ok' | grep -q 1; then
  echo "MongoDB did not become ready at $DB_URI" >&2
  exit 1
fi

if [[ "$RUN_CREDIT_EXCHANGER_SEED" == "true" ]]; then
  echo "Seeding credit-exchanger database..."
  bash scripts/seed-db.sh
fi

credit-exchanger &
credit_exchanger_pid=$!

for _ in {1..60}; do
  if (: >/dev/tcp/127.0.0.1/8080) >/dev/null 2>&1; then
    break
  fi

  if ! kill -0 "$credit_exchanger_pid" 2>/dev/null; then
    wait "$credit_exchanger_pid"
  fi

  sleep 1
done

if ! (: >/dev/tcp/127.0.0.1/8080) >/dev/null 2>&1; then
  echo "credit-exchanger did not become ready at 127.0.0.1:8080" >&2
  exit 1
fi

socat "TCP-LISTEN:${CREDIT_EXCHANGER_PROXY_PORT},fork,reuseaddr,bind=0.0.0.0" TCP:127.0.0.1:8080 &
proxy_pid=$!

wait -n "$credit_exchanger_pid" "$proxy_pid"
