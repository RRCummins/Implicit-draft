#!/bin/sh
set -eu

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
SRC_ROOT="$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(CDPATH= cd -- "$SRC_ROOT/.." && pwd)"

APP_DIR="${1:-$REPO_ROOT/ImplicitDraft-output/Implicit.app}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$SRC_ROOT/Cargo.toml" | head -n 1)"
ARCHIVE_PATH="${2:-$REPO_ROOT/ImplicitDraft-output/Implicit-v$VERSION-macos-arm64.zip}"

"$SCRIPT_DIR/package_app.sh" "$APP_DIR"
rm -f "$ARCHIVE_PATH"

APP_PARENT="$(dirname -- "$APP_DIR")"
APP_NAME="$(basename -- "$APP_DIR")"
(cd "$APP_PARENT" && ditto -c -k --sequesterRsrc --keepParent "$APP_NAME" "$ARCHIVE_PATH")

echo "archived $ARCHIVE_PATH"
