#!/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
SRC_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(CDPATH= cd -- "$SRC_ROOT/.." && pwd)"

APP_DIR="${1:-$REPO_ROOT/ImplicitDraft-output/Implicit.app}"
BIN_PATH="${2:-$REPO_ROOT/ImplicitDraft-output/release/implicit}"

if [ ! -f "$BIN_PATH" ]; then
    echo "missing release binary: $BIN_PATH" >&2
    exit 1
fi

mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$SCRIPT_DIR/Info.plist" "$APP_DIR/Contents/Info.plist"
rm -f "$APP_DIR/Contents/MacOS/Implicit" "$APP_DIR/Contents/MacOS/ImplicitApp" "$APP_DIR/Contents/MacOS/implicit"
cp "$SCRIPT_DIR/ImplicitApp" "$APP_DIR/Contents/MacOS/ImplicitApp"
cp "$BIN_PATH" "$APP_DIR/Contents/MacOS/implicit"
chmod +x "$APP_DIR/Contents/MacOS/ImplicitApp" "$APP_DIR/Contents/MacOS/implicit"

echo "packaged $APP_DIR"
