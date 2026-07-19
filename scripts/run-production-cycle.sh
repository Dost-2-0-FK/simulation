#!/usr/bin/env bash

set -u

usage() {
  echo "Usage: $0 <interval-seconds>" >&2
  echo "Set SIMULATION_URL to override http://127.0.0.1:8080." >&2
  echo "Set CREDIT_SERVICE_URL to override http://127.0.0.1:18080." >&2
  echo "Set COORDINATION_API_KEY to the API key used for authorization." >&2
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
half_interval_seconds=$((interval_seconds / 2))
if (( interval_seconds % 2 != 0 )); then
  half_interval_seconds="${half_interval_seconds}.5"
fi

simulation_url=${SIMULATION_URL:-http://127.0.0.1:8080}
simulation_url=${simulation_url%/}
credit_service_url=${CREDIT_SERVICE_URL:-http://127.0.0.1:18080}
credit_service_url=${credit_service_url%/}

if [[ -z ${COORDINATION_API_KEY:-} ]]; then
  echo "COORDINATION_API_KEY must be set." >&2
  exit 1
fi

coordination_api_key=$COORDINATION_API_KEY

stop() {
  echo
  echo "Production cycle stopped."
  exit 0
}

invoke_endpoint() {
  local base_url=$1
  local endpoint=$2
  local timestamp

  timestamp=$(date '+%Y-%m-%dT%H:%M:%S%z')

  if curl --fail --silent --show-error \
    --header "Authorization: Bearer ${coordination_api_key}" \
    --request POST "${base_url}${endpoint}"; then
    echo "${timestamp} POST ${base_url}${endpoint} succeeded"
  else
    echo "${timestamp} POST ${base_url}${endpoint} failed" >&2
    return 1
  fi
}

run_production_cycle() {
  local failed=0

  invoke_endpoint "$simulation_url" "/api/trusts/publish-production" || failed=1
  invoke_endpoint "$simulation_url" "/api/bases/publish-production" || failed=1
  invoke_endpoint "$simulation_url" "/api/units/produce" || failed=1

  return "$failed"
}

run_credit_evaluation() {
  invoke_endpoint "$credit_service_url" "/api/evaluations/hourly"
}

trap stop INT TERM

echo "Waiting an initial ${interval_seconds}s, then running credit evaluation and production cycle every ${interval_seconds}s, staggered by ${half_interval_seconds}s."
sleep "$interval_seconds"
while true; do
  run_credit_evaluation || true
  sleep "$half_interval_seconds"
  run_production_cycle || true
  sleep "$half_interval_seconds"
done
