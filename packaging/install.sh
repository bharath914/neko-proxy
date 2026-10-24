#!/bin/sh
# Install Neko Proxy for the current user from a GitHub release, no root needed.
#
#   curl -fsSL https://raw.githubusercontent.com/OWNER/REPO/main/packaging/install.sh | sh
#   sh install.sh --version v0.1.0
#   sh install.sh --tarball neko-0.1.0-linux-x86_64.tar.gz     (offline, skips download)
#   sh install.sh --uninstall
#
# Env: NEKO_REPO (default bharath914/neko-proxy), PREFIX (default ~/.local)
set -eu

REPO=${NEKO_REPO:-bharath914/neko-proxy}
PREFIX=${PREFIX:-$HOME/.local}
VERSION=latest
TARBALL=
UNINSTALL=

while [ $# -gt 0 ]; do
  case $1 in
    --version)   VERSION=$2; shift 2 ;;
    --tarball)   TARBALL=$2; shift 2 ;;
    --prefix)    PREFIX=$2; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help)   sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

case $PREFIX in /*) ;; *) PREFIX=$PWD/$PREFIX ;; esac
if printf '%s' "$PREFIX" | LC_ALL=C grep -q '[[:cntrl:]]'; then
  echo "install prefix must not contain control characters" >&2
  exit 2
fi

APPS=$PREFIX/share/applications
ICONS=$PREFIX/share/icons/hicolor/512x512/apps

refresh() {
  command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
}

if [ -n "$UNINSTALL" ]; then
  rm -f "$PREFIX/bin/neko" "$PREFIX/bin/nekod" "$APPS/dev.nekoproxy.Neko.desktop" "$ICONS/dev.nekoproxy.Neko.png"
  refresh
  echo "Removed Neko Proxy from $PREFIX. Captures and the CA stay in ~/.local/share/neko."
  exit 0
fi

case $(uname -m) in
  x86_64)        ARCH=x86_64 ;;
  aarch64|arm64) ARCH=aarch64 ;;
  *) echo "unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac
[ "$(uname -s)" = Linux ] || { echo "Neko Proxy runs on Linux only" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

if [ -z "$TARBALL" ]; then
  command -v curl >/dev/null 2>&1 || { echo "curl is required" >&2; exit 1; }
  if [ "$VERSION" = latest ]; then
    # /releases/latest redirects to /releases/tag/<tag>.
    VERSION=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")
    VERSION=${VERSION##*/}
    case $VERSION in v*) ;; *) echo "could not find a release of $REPO" >&2; exit 1 ;; esac
  fi
  NAME=neko-${VERSION#v}-linux-$ARCH.tar.gz
  BASE=https://github.com/$REPO/releases/download/$VERSION
  echo "Downloading $NAME"
  curl -fSL --progress-bar -o "$TMP/$NAME" "$BASE/$NAME"
  curl -fsSL -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS"
  WANT=$(grep " $NAME\$" "$TMP/SHA256SUMS" | cut -d' ' -f1)
  GOT=$(sha256sum "$TMP/$NAME" | cut -d' ' -f1)
  [ -n "$WANT" ] && [ "$WANT" = "$GOT" ] || { echo "checksum mismatch for $NAME" >&2; exit 1; }
  TARBALL=$TMP/$NAME
else
  echo "Installing from $TARBALL (checksum not verified)"
fi

tar -xzf "$TARBALL" -C "$TMP"
SRC=$(find "$TMP" -maxdepth 1 -type d -name 'neko-*-linux-*' | head -1)
[ -x "$SRC/bin/neko" ] || { echo "unexpected tarball layout" >&2; exit 1; }

mkdir -p "$PREFIX/bin" "$APPS" "$ICONS"
# Install over a running copy safely: write a new file, then rename.
for b in neko nekod; do
  install -m 0755 "$SRC/bin/$b" "$PREFIX/bin/.$b.new"
  mv -f "$PREFIX/bin/.$b.new" "$PREFIX/bin/$b"
done
install -m 0644 "$SRC/share/icons/hicolor/512x512/apps/dev.nekoproxy.Neko.png" "$ICONS/"
# Absolute, quoted Exec also supports prefixes containing spaces or punctuation.
DESKTOP_EXEC=$(printf '%s/bin/neko' "$PREFIX" | sed 's/[\\"`$]/\\&/g')
SED_EXEC=$(printf '%s' "$DESKTOP_EXEC" | sed 's/[\\&|]/\\&/g')
sed "s|^Exec=neko\$|Exec=\"$SED_EXEC\"|" "$SRC/share/applications/dev.nekoproxy.Neko.desktop" \
  > "$APPS/dev.nekoproxy.Neko.desktop"
refresh

echo "Installed to $PREFIX. Start it from your app menu or run: $PREFIX/bin/neko"
case ":$PATH:" in *":$PREFIX/bin:"*) ;; *) echo "Note: $PREFIX/bin is not on your PATH." ;; esac
echo "First launch: Settings > Certificate to trust the proxy CA."
