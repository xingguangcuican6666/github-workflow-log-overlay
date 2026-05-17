#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(node -e "process.stdout.write(JSON.parse(require('fs').readFileSync('package.json', 'utf8')).version)")"
TARGET="${1:-linux-x86_64}"
APP_ID="dev.github-overlay.workflow-log"
NAME="github-workflow-log-overlay"
OUT_DIR="$ROOT_DIR/dist-portable/$NAME-$VERSION-$TARGET"
ARCHIVE="$ROOT_DIR/dist-portable/$NAME-$VERSION-$TARGET.tar.gz"

if [[ ! -x "$ROOT_DIR/src-tauri/target/release/$NAME" ]]; then
  echo "Missing release binary. Run: npm run tauri -- build --no-bundle" >&2
  exit 1
fi

rm -rf "$OUT_DIR" "$ARCHIVE"
mkdir -p \
  "$OUT_DIR/bin" \
  "$OUT_DIR/share/applications" \
  "$OUT_DIR/share/icons/hicolor/128x128/apps" \
  "$OUT_DIR/share/doc/$NAME"

install -m755 "$ROOT_DIR/src-tauri/target/release/$NAME" "$OUT_DIR/bin/$NAME"
install -m644 "$ROOT_DIR/packaging/linux/$APP_ID.desktop" "$OUT_DIR/share/applications/$APP_ID.desktop"
install -m644 "$ROOT_DIR/src-tauri/icons/128x128.png" "$OUT_DIR/share/icons/hicolor/128x128/apps/$APP_ID.png"
install -m644 "$ROOT_DIR/README.md" "$OUT_DIR/share/doc/$NAME/README.md"
install -m644 "$ROOT_DIR/LICENSE" "$OUT_DIR/share/doc/$NAME/LICENSE"

cat > "$OUT_DIR/install.sh" <<'INSTALL'
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"

mkdir -p \
  "$PREFIX/bin" \
  "$PREFIX/share/applications" \
  "$PREFIX/share/icons/hicolor/128x128/apps" \
  "$PREFIX/share/doc/github-workflow-log-overlay"

install -m755 "$ROOT_DIR/bin/github-workflow-log-overlay" "$PREFIX/bin/github-workflow-log-overlay"
install -m644 "$ROOT_DIR/share/applications/dev.github-overlay.workflow-log.desktop" \
  "$PREFIX/share/applications/dev.github-overlay.workflow-log.desktop"
install -m644 "$ROOT_DIR/share/icons/hicolor/128x128/apps/dev.github-overlay.workflow-log.png" \
  "$PREFIX/share/icons/hicolor/128x128/apps/dev.github-overlay.workflow-log.png"
install -m644 "$ROOT_DIR/share/doc/github-workflow-log-overlay/README.md" \
  "$PREFIX/share/doc/github-workflow-log-overlay/README.md"
install -m644 "$ROOT_DIR/share/doc/github-workflow-log-overlay/LICENSE" \
  "$PREFIX/share/doc/github-workflow-log-overlay/LICENSE"

echo "Installed to $PREFIX. Ensure $PREFIX/bin is in PATH."
INSTALL

chmod +x "$OUT_DIR/install.sh"
tar -C "$ROOT_DIR/dist-portable" -czf "$ARCHIVE" "$(basename "$OUT_DIR")"
echo "$ARCHIVE"

