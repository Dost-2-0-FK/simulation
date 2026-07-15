#!/usr/bin/env bash

set -u

usage() {
  echo "Usage: $0 <interval-seconds>" >&2
  echo "Set SIMULATION_URL to override http://127.0.0.1:8080." >&2
}

if [[ $# -ne 1 || ! $1 =~ ^[1-9][0-9]*$ ]]; then
  usage
  exit 2
fi

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required but was not found." >&2
  exit 1
fi

interval_seconds=$1
simulation_url=${SIMULATION_URL:-http://127.0.0.1:8080}
simulation_url=${simulation_url%/}

stop() {
  echo
  echo "Production cycle stopped."
  exit 0
}

invoke_endpoint() {
  local endpoint=$1
  local timestamp

  timestamp=$(date '+%Y-%m-%dT%H:%M:%S%z')

  if curl --fail --silent --show-error --request POST "${simulation_url}${endpoint}"; then
    echo "${timestamp} POST ${endpoint} succeeded"
  else
    echo "${timestamp} POST ${endpoint} failed" >&2
    return 1
  fi
}

run_cycle() {
  local failed=0

  invoke_endpoint "/api/trusts/publish-production" || failed=1
  invoke_endpoint "/api/bases/publish-production" || failed=1
  invoke_endpoint "/api/units/produce" || failed=1

  return "$failed"
}

trap stop INT TERM

echo "Running production cycle every ${interval_seconds}s against ${simulation_url}."
while true; do
  run_cycle || true
  sleep "$interval_seconds"
done
