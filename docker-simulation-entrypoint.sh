#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${COORDINATION_API_KEY:-}" ]]; then
    echo "Error: COORDINATION_API_KEY must be set at runtime." >&2
    exit 1
fi

wait_for_tcp() {
  local host="$1"
  local port="$2"
  local name="$3"

  for _ in {1..60}; do
    if (: >"/dev/tcp/${host}/${port}") >/dev/null 2>&1; then
      return 0
    fi

    sleep 1
  done

  echo "$name did not become ready at ${host}:${port}" >&2
  return 1
}

wait_for_tcp "${MONGODB_HOST:-mongodb}" "${MONGODB_PORT:-27017}" "MongoDB"
wait_for_tcp "${CREDIT_EXCHANGER_HOST:-credit-exchanger}" "${CREDIT_EXCHANGER_PORT:-18080}" "credit-exchanger"

exec simulation
