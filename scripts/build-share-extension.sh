#!/usr/bin/env bash
set -euo pipefail

# Builds the macOS Share-sheet extension and embeds it into a WireHop.app
# bundle. The appex is signed ad-hoc WITH its sandbox entitlements; callers
# that later re-sign the outer app must not use --deep, which would strip
# those entitlements and make pluginkit reject the extension.

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
SRC_DIR="$REPO_ROOT/WireHop/shareext"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "build-share-extension.sh only runs on macOS." >&2
    exit 2
fi

APP=${1:?usage: build-share-extension.sh <path/to/WireHop.app>}
if [[ ! -d "$APP/Contents/MacOS" ]]; then
    echo "$APP does not look like an application bundle." >&2
    exit 2
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

clang -fobjc-arc -fapplication-extension -mmacosx-version-min=10.15 \
    -c "$SRC_DIR/ShareViewController.m" -o "$WORK/ShareViewController.o"
clang -o "$WORK/WireHopShare" "$WORK/ShareViewController.o" \
    -fapplication-extension -mmacosx-version-min=10.15 \
    -framework Foundation -framework AppKit -Wl,-e,_NSExtensionMain

APPEX="$APP/Contents/PlugIns/WireHopShare.appex"
rm -rf "$APPEX"
mkdir -p "$APPEX/Contents/MacOS"
cp "$SRC_DIR/Info.plist" "$APPEX/Contents/Info.plist"
cp "$WORK/WireHopShare" "$APPEX/Contents/MacOS/WireHopShare"

codesign --force --sign - --entitlements "$SRC_DIR/entitlements.plist" "$APPEX"
echo "Embedded and signed $APPEX"
