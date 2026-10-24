#!/bin/sh
# Install Neko Proxy on macOS from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/bharath914/neko-proxy/main/packaging/install-mac.sh | sh
#   sh install-mac.sh --version v0.1.1
#   sh install-mac.sh --dest ~/Applications
#   sh install-mac.sh --uninstall
#
# Downloads the universal disk image, checks it against the release's SHA256SUMS
# and copies Neko.app into /Applications (or ~/Applications if that isn't writable).
# Env: NEKO_REPO (default bharath914/neko-proxy)
set -eu

REPO=${NEKO_REPO:-bharath914/neko-proxy}
VERSION=latest
DEST=
UNINSTALL=

while [ $# -gt 0 ]; do
  case $1 in
    --version)   VERSION=$2; shift 2 ;;
    --dest)      DEST=$2; shift 2 ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help)   sed -n '2,11p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

[ "$(uname -s)" = Darwin ] || { echo "This installer is for macOS. On Linux use packaging/install.sh." >&2; exit 1; }
MAJOR=$(sw_vers -productVersion | cut -d. -f1)
[ "$MAJOR" -ge 13 ] || { echo "Neko needs macOS 13 or later (this Mac has $(sw_vers -productVersion))." >&2; exit 1; }

if [ -z "$DEST" ]; then
  if [ -w /Applications ]; then DEST=/Applications; else DEST=$HOME/Applications; fi
fi
APP=$DEST/Neko.app

if [ -n "$UNINSTALL" ]; then
  rm -rf "$APP"
  echo "Removed $APP. Captures and the CA stay in ~/Library/Application Support/neko."
  exit 0
fi

TMP=$(mktemp -d)
MNT=$TMP/mnt
cleanup() {
  [ -d "$MNT" ] && hdiutil detach -quiet "$MNT" 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

if [ "$VERSION" = latest ]; then
  # /releases/latest redirects to /releases/tag/<tag>.
  VERSION=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "https://github.com/$REPO/releases/latest")
  VERSION=${VERSION##*/}
  case $VERSION in v*) ;; *) echo "could not find a release of $REPO" >&2; exit 1 ;; esac
fi
NAME=Neko-${VERSION#v}-macos-universal.dmg
BASE=https://github.com/$REPO/releases/download/$VERSION

echo "Downloading $NAME"
curl -fSL --progress-bar -o "$TMP/$NAME" "$BASE/$NAME"
curl -fsSL -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS"
WANT=$(grep " $NAME\$" "$TMP/SHA256SUMS" | cut -d' ' -f1)
GOT=$(shasum -a 256 "$TMP/$NAME" | cut -d' ' -f1)
[ -n "$WANT" ] && [ "$WANT" = "$GOT" ] || { echo "checksum mismatch for $NAME" >&2; exit 1; }

mkdir -p "$MNT" "$DEST"
hdiutil attach -quiet -nobrowse -readonly -mountpoint "$MNT" "$TMP/$NAME"
codesign --verify --deep --strict "$MNT/Neko.app"
# Install over an existing copy safely: copy beside it, then swap.
rm -rf "$APP.new"
ditto "$MNT/Neko.app" "$APP.new"
rm -rf "$APP"
mv "$APP.new" "$APP"

echo "Installed Neko ${VERSION#v} to $APP"
echo "Open it from Launchpad or Spotlight, or run: open \"$APP\""
