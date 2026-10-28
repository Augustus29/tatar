#!/bin/sh
# Tatar macOS kurulum betiği.
#
# Kullanım:
#   curl -fsSL https://raw.githubusercontent.com/Augustus29/tatar/main/scripts/install.sh | sh
#
# GitHub'daki son sürümün Tatar.app.zip dosyasını indirir, varsa SHA256SUMS ile doğrular
# ve /Applications (yazılamıyorsa ~/Applications) klasörüne kurar. Terminal'den indirilen
# dosyalara macOS "internetten indirildi" işareti koymadığı için imzasız uygulama uyarısı çıkmaz.
# Yine de olası bir işareti temizlemek için xattr çalıştırılır.

set -eu

REPO="Augustus29/tatar"
BASE_URL="https://github.com/$REPO/releases/latest/download"
APP_NAME="Tatar.app"
ZIP_NAME="Tatar.app.zip"

say() { printf '%s\n' "$*"; }
fail() { printf 'Hata / Error: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || fail "Bu betik yalnızca macOS içindir. / This script is for macOS only."

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM

say "Tatar indiriliyor... / Downloading Tatar..."
curl -fL --progress-bar -o "$TMP_DIR/$ZIP_NAME" "$BASE_URL/$ZIP_NAME" \
  || fail "İndirme başarısız. / Download failed."

# Eski sürümlerde SHA256SUMS yok; varsa özet eşleşmek zorunda.
if curl -fsL -o "$TMP_DIR/SHA256SUMS" "$BASE_URL/SHA256SUMS" 2>/dev/null; then
  expected="$(awk -v f="$ZIP_NAME" '$2 == f { print $1 }' "$TMP_DIR/SHA256SUMS")"
  [ -n "$expected" ] || fail "SHA256SUMS içinde $ZIP_NAME yok. / $ZIP_NAME missing from SHA256SUMS."
  actual="$(shasum -a 256 "$TMP_DIR/$ZIP_NAME" | awk '{ print $1 }')"
  [ "$expected" = "$actual" ] || fail "Dosya doğrulanamadı (SHA-256 uyuşmuyor). / Checksum mismatch."
  say "Dosya doğrulandı. / Checksum verified."
fi

ditto -x -k "$TMP_DIR/$ZIP_NAME" "$TMP_DIR/unzipped" || fail "Arşiv açılamadı. / Could not extract archive."
[ -d "$TMP_DIR/unzipped/$APP_NAME" ] || fail "Arşivde $APP_NAME yok. / $APP_NAME not found in archive."

if [ -w /Applications ]; then
  DEST_DIR="/Applications"
else
  DEST_DIR="$HOME/Applications"
  mkdir -p "$DEST_DIR"
fi

# Çalışıyorsa kapat, yoksa eski sürümün üzerine yazılamaz.
if pgrep -xq tatar || pgrep -xq Tatar; then
  say "Açık olan Tatar kapatılıyor... / Quitting running Tatar..."
  osascript -e 'quit app "Tatar"' >/dev/null 2>&1 || true
  sleep 2
fi

rm -rf "${DEST_DIR:?}/$APP_NAME"
ditto "$TMP_DIR/unzipped/$APP_NAME" "$DEST_DIR/$APP_NAME"
xattr -dr com.apple.quarantine "$DEST_DIR/$APP_NAME" 2>/dev/null || true

say "Tatar kuruldu: $DEST_DIR/$APP_NAME / Installed."
open "$DEST_DIR/$APP_NAME"
