#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$SCRIPT_DIR/_common.sh"

TEST_BUILD_DIR=${WIREHOP_TEST_BUILD_DIR:-${LANDROP_TEST_BUILD_DIR:-"$REPO_ROOT/build-agent-tests"}}
qmake_bin=$(find_qmake)
qt_version=$("$qmake_bin" -query QT_VERSION)
if [[ "$qt_version" != 5.* ]]; then
    echo "WireHop's reference tests require Qt 5; $qmake_bin reports Qt $qt_version." >&2
    exit 2
fi

# Build the Rust peer first: the Qt interop suite drives it as a subprocess.
# When no toolchain is present run_cargo skips, WIREHOP_CLI_BIN stays unset,
# and the interop cases skip too (or fail, under WIREHOP_REQUIRE_RUST=1).
run_cargo build -p wirehop-cli
cli_bin="$CORE_DIR/target/debug/wirehop-cli"
if [[ -x "$cli_bin" ]]; then
    export WIREHOP_CLI_BIN="$cli_bin"
fi

mkdir -p "$TEST_BUILD_DIR"
qmake_args=("$REPO_ROOT/tests/tests.pro")
if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists libsodium; then
    sodium_include=$(pkg-config --variable=includedir libsodium)
    sodium_lib=$(pkg-config --variable=libdir libsodium)
    qmake_args+=("INCLUDEPATH+=$sodium_include" "LIBS+=-L$sodium_lib")
fi
(cd "$TEST_BUILD_DIR" && "$qmake_bin" "${qmake_args[@]}")
make -C "$TEST_BUILD_DIR" -j"$(build_jobs)"
"$TEST_BUILD_DIR/wirehop_tests" -txt
run_cargo test
