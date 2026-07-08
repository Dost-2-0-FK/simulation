#!/usr/bin/env bash
set -euo pipefail

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
