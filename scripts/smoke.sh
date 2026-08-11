#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$SCRIPT_DIR/_common.sh"

build_landrop
binary=$(landrop_binary)
smoke_log="$BUILD_DIR/smoke.log"
smoke_seconds=${LANDROP_SMOKE_SECONDS:-2}

"$binary" >"$smoke_log" 2>&1 &
landrop_pid=$!

cleanup() {
    if kill -0 "$landrop_pid" 2>/dev/null; then
        kill "$landrop_pid" 2>/dev/null || true
        wait "$landrop_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

sleep "$smoke_seconds"
if ! kill -0 "$landrop_pid" 2>/dev/null; then
    wait "$landrop_pid" || status=$?
    status=${status:-0}
    echo "LANDrop exited during the ${smoke_seconds}s startup window (status $status)." >&2
    sed -n '1,120p' "$smoke_log" >&2
    exit 1
fi

echo "LANDrop remained running for ${smoke_seconds}s; native startup smoke check passed."
