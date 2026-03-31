#!/bin/zsh
set -euo pipefail

if [[ $# -lt 1 || $# -gt 3 ]]; then
  echo "usage: $0 /path/to/implicit.app [output.dmg] [volume-name]" >&2
  exit 1
fi

APP_PATH=$1
OUTPUT_PATH=${2:-"$(pwd)/Implicit.dmg"}
VOLUME_NAME=${3:-"Implicit"}

if [[ ! -d "$APP_PATH" || "${APP_PATH:t:e}" != "app" ]]; then
  echo "error: app bundle not found: $APP_PATH" >&2
  exit 1
fi

TMP_DIR=$(mktemp -d /tmp/implicit-dmg.XXXXXX)
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

cp -R "$APP_PATH" "$TMP_DIR/"
mkdir -p "${OUTPUT_PATH:h}"
rm -f "$OUTPUT_PATH"

hdiutil create \
  -volname "$VOLUME_NAME" \
  -srcfolder "$TMP_DIR" \
  -ov \
  -format UDZO \
  "$OUTPUT_PATH"

echo "created $OUTPUT_PATH"
