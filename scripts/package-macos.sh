#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$SCRIPT_DIR/_common.sh"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "package-macos.sh only runs on macOS." >&2
    exit 2
fi

build_wirehop

if [[ ! -d "$BUILD_DIR/WireHop.app" ]]; then
    echo "WireHop.app was not found under $BUILD_DIR after the build." >&2
    exit 1
fi

qmake_bin=$(find_qmake)
macdeployqt_bin="$(dirname "$qmake_bin")/macdeployqt"
if [[ ! -x "$macdeployqt_bin" ]]; then
    macdeployqt_bin=$(command -v macdeployqt) || {
        echo "macdeployqt was not found next to $qmake_bin or on PATH." >&2
        exit 2
    }
fi

DIST_DIR=${WIREHOP_DIST_DIR:-"$REPO_ROOT/dist-macos"}
APP="$DIST_DIR/WireHop.app"
ARCH=${WIREHOP_MACOS_ARCH:-arm64}

# Deploy into a staging copy only; macdeployqt must never touch $BUILD_DIR.
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"
ditto "$BUILD_DIR/WireHop.app" "$APP"

"$macdeployqt_bin" "$APP"

# macdeployqt invalidates the linker's ad-hoc signature; re-sign and verify,
# otherwise the packaged app is killed at launch on Apple Silicon.
codesign --force --deep --sign - "$APP"
# --deep re-signs nested code without its entitlements, which breaks the
# share extension; rebuild/re-sign the appex, then reseal only the outer app.
"$SCRIPT_DIR/build-share-extension.sh" "$APP"
codesign --force --sign - "$APP"
codesign --verify --deep --strict "$APP"

file "$APP/Contents/MacOS/WireHop" | grep -q "$ARCH"
otool -L "$APP/Contents/MacOS/WireHop" | grep -q '@executable_path/../Frameworks/libsodium'

launch_log="$DIST_DIR/launch.log"
"$APP/Contents/MacOS/WireHop" >"$launch_log" 2>&1 &
app_pid=$!
cleanup() {
    if kill -0 "$app_pid" 2>/dev/null; then
        kill "$app_pid" 2>/dev/null || true
        wait "$app_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM
sleep "${WIREHOP_SMOKE_SECONDS:-2}"
if ! kill -0 "$app_pid" 2>/dev/null; then
    wait "$app_pid" || status=$?
    status=${status:-0}
    echo "Packaged WireHop exited during the launch check (status $status)." >&2
    sed -n '1,120p' "$launch_log" >&2
    exit 1
fi
cleanup
trap - EXIT INT TERM

ditto -c -k --sequesterRsrc --keepParent "$APP" "$DIST_DIR/WireHop-macos-$ARCH.zip"
echo "Packaged, ad-hoc signed, verified, and launch-checked: $DIST_DIR/WireHop-macos-$ARCH.zip"
