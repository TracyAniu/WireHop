#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
BUILD_DIR=${LANDROP_BUILD_DIR:-"$REPO_ROOT/build-agent"}
PROJECT_FILE="$REPO_ROOT/LANDrop/LANDrop.pro"

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

configure_landrop() {
    local qmake_bin
    qmake_bin=$(find_qmake)

    local qt_version
    qt_version=$("$qmake_bin" -query QT_VERSION)
    if [[ "$qt_version" != 5.* ]]; then
        echo "LANDrop's reference build requires Qt 5; $qmake_bin reports Qt $qt_version." >&2
        return 2
    fi

    if ! command -v make >/dev/null 2>&1; then
        echo "make was not found. Install a platform build tool before building LANDrop." >&2
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
    local jobs=${LANDROP_JOBS:-}
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

build_landrop() {
    configure_landrop

    local jobs
    jobs=$(build_jobs)

    make -C "$BUILD_DIR" -j"$jobs"
}

landrop_binary() {
    local candidates=(
        "$BUILD_DIR/LANDrop.app/Contents/MacOS/LANDrop"
        "$BUILD_DIR/LANDrop"
        "$BUILD_DIR/landrop"
    )
    local candidate
    for candidate in "${candidates[@]}"; do
        if [[ -x "$candidate" ]]; then
            printf '%s\n' "$candidate"
            return
        fi
    done

    echo "The LANDrop executable was not found under $BUILD_DIR after a successful build." >&2
    return 1
}
