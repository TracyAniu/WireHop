#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$SCRIPT_DIR/_common.sh"

build_wirehop
binary=$(wirehop_binary)
smoke_log="$BUILD_DIR/smoke.log"
smoke_seconds=${WIREHOP_SMOKE_SECONDS:-${LANDROP_SMOKE_SECONDS:-2}}

"$binary" >"$smoke_log" 2>&1 &
wirehop_pid=$!

cleanup() {
    if kill -0 "$wirehop_pid" 2>/dev/null; then
        kill "$wirehop_pid" 2>/dev/null || true
        wait "$wirehop_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

sleep "$smoke_seconds"
if ! kill -0 "$wirehop_pid" 2>/dev/null; then
    wait "$wirehop_pid" || status=$?
    status=${status:-0}
    echo "WireHop exited during the ${smoke_seconds}s startup window (status $status)." >&2
    sed -n '1,120p' "$smoke_log" >&2
    exit 1
fi

echo "WireHop remained running for ${smoke_seconds}s; native startup smoke check passed."
