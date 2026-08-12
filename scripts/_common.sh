#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
BUILD_DIR=${WIREHOP_BUILD_DIR:-${LANDROP_BUILD_DIR:-"$REPO_ROOT/build-agent"}}
PROJECT_FILE="$REPO_ROOT/WireHop/WireHop.pro"

find_qmake() {
    if [[ -n "${QMAKE_BIN:-}" ]]; then
        if [[ ! -x "$QMAKE_BIN" ]]; then
            echo "QMAKE_BIN is not executable: $QMAKE_BIN" >&2
            return 2
        fi
        printf '%s\n' "$QMAKE_BIN"
        return
    fi

    if command -v qmake >/dev/null 2>&1; then
        command -v qmake
        return
    fi

    echo "Qt 5 qmake was not found. Install Qt 5 or set QMAKE_BIN to its qmake executable." >&2
    return 2
}

configure_wirehop() {
    # qmake's generated bundle-plist rule has no dependency on the source
    # Info.plist; drop the stale copy so edits actually reach the bundle.
    if [[ -f "$BUILD_DIR/WireHop.app/Contents/Info.plist" \
            && "$REPO_ROOT/WireHop/Info.plist" -nt "$BUILD_DIR/WireHop.app/Contents/Info.plist" ]]; then
        rm -f "$BUILD_DIR/WireHop.app/Contents/Info.plist"
    fi

    if [[ -f "$BUILD_DIR/WireHop.app/Contents/Resources/qt.conf" ]]; then
        echo "The build directory $BUILD_DIR contains a macdeployqt-processed bundle and cannot be reused." >&2
        echo "Delete it (rm -rf \"$BUILD_DIR\") or point WIREHOP_BUILD_DIR at a clean directory. Packaging must deploy into a staging copy, never into the build directory." >&2
        return 2
    fi

    local qmake_bin
    qmake_bin=$(find_qmake)

    local qt_version
    qt_version=$("$qmake_bin" -query QT_VERSION)
    if [[ "$qt_version" != 5.* ]]; then
        echo "WireHop's reference build requires Qt 5; $qmake_bin reports Qt $qt_version." >&2
        return 2
    fi

    if ! command -v make >/dev/null 2>&1; then
        echo "make was not found. Install a platform build tool before building WireHop." >&2
        return 2
    fi

    mkdir -p "$BUILD_DIR"
    local qmake_args=("$PROJECT_FILE")

    if command -v pkg-config >/dev/null 2>&1 && pkg-config --exists libsodium; then
        local sodium_include sodium_lib
        sodium_include=$(pkg-config --variable=includedir libsodium)
        sodium_lib=$(pkg-config --variable=libdir libsodium)
        qmake_args+=("INCLUDEPATH+=$sodium_include" "LIBS+=-L$sodium_lib")
    fi

    (cd "$BUILD_DIR" && "$qmake_bin" "${qmake_args[@]}")
}

build_jobs() {
    local jobs=${WIREHOP_JOBS:-${LANDROP_JOBS:-}}
    if [[ -z "$jobs" ]]; then
        if command -v nproc >/dev/null 2>&1; then
            jobs=$(nproc)
        elif command -v sysctl >/dev/null 2>&1; then
            jobs=$(sysctl -n hw.ncpu)
        else
            jobs=2
        fi
    fi

    printf '%s\n' "$jobs"
}

build_wirehop() {
    configure_wirehop

    local jobs
    jobs=$(build_jobs)

    make -C "$BUILD_DIR" -j"$jobs"
}

wirehop_binary() {
    local candidates=(
        "$BUILD_DIR/WireHop.app/Contents/MacOS/WireHop"
        "$BUILD_DIR/WireHop"
        "$BUILD_DIR/wirehop"
    )
    local candidate
    for candidate in "${candidates[@]}"; do
        if [[ -x "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return
        fi
    done

    echo "The WireHop executable was not found under $BUILD_DIR after a successful build." >&2
    return 1
}

# --- Rust core -------------------------------------------------------------
# The Rust core (core/) is a second implementation of the wire protocol, held
# to the C++/Qt baseline by the conformance fixture. Contributors without a
# Rust toolchain must still be able to build and test the Qt application, so
# these helpers skip loudly rather than fail. CI sets WIREHOP_REQUIRE_RUST=1 so
# a missing toolchain there is an error, never a silent loss of coverage.

CORE_DIR="$REPO_ROOT/core"

find_cargo() {
    if [[ -n "${CARGO_BIN:-}" ]]; then
        printf '%s\n' "$CARGO_BIN"
        return 0
    fi
    if command -v cargo >/dev/null 2>&1; then
        command -v cargo
        return 0
    fi
    if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
        printf '%s\n' "$HOME/.cargo/bin/cargo"
        return 0
    fi
    return 1
}

# Runs `cargo "$@"` in core/, or reports a skip. Returns non-zero only when the
# toolchain is required but absent, or when cargo itself fails.
run_cargo() {
    local cargo_bin
    if ! cargo_bin=$(find_cargo); then
        if [[ "${WIREHOP_REQUIRE_RUST:-0}" == "1" ]]; then
            echo "Rust toolchain not found and WIREHOP_REQUIRE_RUST=1." >&2
            echo "Install it with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
            return 2
        fi
        echo "SKIPPING Rust core checks: no cargo on PATH (set CARGO_BIN, or install rustup)." >&2
        echo "  The Qt application was still checked. Set WIREHOP_REQUIRE_RUST=1 to make this fatal." >&2
        return 0
    fi
    (cd "$CORE_DIR" && "$cargo_bin" "$@")
}
