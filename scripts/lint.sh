#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$SCRIPT_DIR/_common.sh"

bash -n "$SCRIPT_DIR"/*.sh
python3 -m json.tool "$REPO_ROOT/docs/agent-harness/features.json" >/dev/null

if rg -n '[[:blank:]]+$' "$REPO_ROOT/AGENTS.md" "$REPO_ROOT/docs" "$REPO_ROOT/scripts"; then
    echo "Trailing whitespace found in harness files." >&2
    exit 1
fi

git -C "$REPO_ROOT" diff --check -- .

run_cargo fmt --all -- --check
run_cargo clippy --all-targets -- -D warnings
