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

wait_for_uri() {
  local uri="$1"
  local default_port="$2"
  local name="$3"
  local authority host port

  authority="${uri#*://}"
  authority="${authority%%[/?#]*}"
  authority="${authority##*@}"
  authority="${authority%%,*}"

  if [[ "$authority" =~ ^\[([^]]+)\](:([0-9]+))?$ ]]; then
    host="${BASH_REMATCH[1]}"
    port="${BASH_REMATCH[3]:-$default_port}"
  elif [[ "$authority" =~ ^([^:]+)(:([0-9]+))?$ ]]; then
    host="${BASH_REMATCH[1]}"
    port="${BASH_REMATCH[3]:-$default_port}"
  else
    echo "Unable to determine the $name endpoint from $uri" >&2
    return 1
  fi

  wait_for_tcp "$host" "$port" "$name"
}

wait_for_uri "${MONGODB_URI:-mongodb://mongodb:27017}" 27017 "MongoDB"

credit_exchange_url="${CREDIT_EXCHANGE_URL:-http://credit-exchanger:18080}"
case "$credit_exchange_url" in
  https://*) credit_exchange_default_port=443 ;;
  *) credit_exchange_default_port=80 ;;
esac
wait_for_uri "$credit_exchange_url" "$credit_exchange_default_port" "credit-exchanger"

exec simulation "$@"
