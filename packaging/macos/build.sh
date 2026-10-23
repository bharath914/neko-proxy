#!/usr/bin/env bash
# Build the macOS installer into dist/:
#   Neko.app                       the app, with nekod inside (Contents/MacOS/nekod)
#   Neko-<ver>-macos-<arch>.dmg    drag-to-Applications disk image
#   Neko-<ver>-macos-<arch>.dmg.sha256
#
# Needs the Command Line Tools (swift, hdiutil, iconutil, codesign) and Rust. Builds for this
# Mac's architecture only.
#
# Signing, in order of preference:
#   SIGN_IDENTITY="Developer ID Application: Name (TEAMID)"   sign with your certificate and the hardened runtime
#   (unset)                                                    ad-hoc signature; first launch needs right-click > Open
# Notarization (needs SIGN_IDENTITY): NOTARY_PROFILE=<name from `xcrun notarytool store-credentials`>
#
# Env: NEKO_SKIP_BUILD=1 reuses the existing release builds.
set -euo pipefail

cd "$(dirname "$0")/../.."
ROOT=$PWD
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=$(uname -m)
OUT=$ROOT/dist
APP=$OUT/Neko.app
DMG=$OUT/Neko-$VERSION-macos-$ARCH.dmg
export PATH="$HOME/.cargo/bin:$PATH"

if [ -z "${NEKO_SKIP_BUILD:-}" ]; then
  echo "==> building nekod"
  cargo build --release -p neko-cli
  echo "==> building the app"
  (cd desktop-macos && swift build -c release --product Neko)
fi
SWIFT_BIN=$(cd desktop-macos && swift build -c release --show-bin-path)

rm -rf "$APP" "$OUT"/*.dmg "$OUT"/*.sha256 "$OUT/dmg"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 0755 "$SWIFT_BIN/Neko" "$APP/Contents/MacOS/Neko"
install -m 0755 target/release/nekod "$APP/Contents/MacOS/nekod"
cp -R desktop-macos/Sources/NekoApp/Resources/Fonts "$APP/Contents/Resources/Fonts"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist.in > "$APP/Contents/Info.plist"

echo "==> icon"
ICONSET=$OUT/Neko.iconset
swift packaging/macos/make-icon.swift "$ICONSET"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Neko.icns"
rm -rf "$ICONSET"

echo "==> signing"
if [ -n "${SIGN_IDENTITY:-}" ]; then
  codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP/Contents/MacOS/nekod"
  codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
else
  codesign --force --sign - "$APP/Contents/MacOS/nekod"
  codesign --force --sign - "$APP"
fi
codesign --verify --strict "$APP"

echo "==> disk image"
STAGE=$OUT/dmg
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/Neko.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -volname "Neko" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
rm -rf "$STAGE"
[ -n "${SIGN_IDENTITY:-}" ] && codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"

if [ -n "${NOTARY_PROFILE:-}" ]; then
  [ -n "${SIGN_IDENTITY:-}" ] || { echo "notarization needs SIGN_IDENTITY" >&2; exit 1; }
  echo "==> notarizing (this waits for Apple)"
  xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple "$DMG"
fi

(cd "$OUT" && shasum -a 256 "$(basename "$DMG")" > "$(basename "$DMG").sha256")
echo
echo "built $DMG"
[ -z "${SIGN_IDENTITY:-}" ] && echo "ad-hoc signed: on another Mac, right-click Neko.app > Open the first time (or run: xattr -dr com.apple.quarantine /Applications/Neko.app)"
exit 0
