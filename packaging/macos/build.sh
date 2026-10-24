#!/usr/bin/env bash
# Build the macOS installer into dist/:
#   Neko.app                       the app, with nekod inside (Contents/MacOS/nekod)
#   Neko-<ver>-macos-<arch>.dmg    drag-to-Applications disk image
#   Neko-<ver>-macos-<arch>.dmg.sha256
#
# Needs the Command Line Tools (swift, hdiutil, iconutil, codesign) and Rust. Builds for this
# Mac's architecture, or both Apple silicon and Intel with UNIVERSAL=1 (needs the Intel Rust
# target: `rustup target add x86_64-apple-darwin`, and the arm64 one on an Intel Mac).
#
# Signing, in order of preference:
#   SIGN_IDENTITY="Developer ID Application: Name (TEAMID)"   sign with your certificate and the hardened runtime
#   (unset)                                                    ad-hoc signature; first launch needs right-click > Open
# Notarization (needs SIGN_IDENTITY): NOTARY_PROFILE=<name from `xcrun notarytool store-credentials`>
#
# Env: NEKO_SKIP_BUILD=1 reuses the existing release builds. UNIVERSAL=1 builds a fat app and nekod.
set -euo pipefail

cd "$(dirname "$0")/../.."
ROOT=$PWD
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=$(uname -m)
[ -n "${UNIVERSAL:-}" ] && ARCH=universal
OUT=$ROOT/dist
APP=$OUT/Neko.app
DMG=$OUT/Neko-$VERSION-macos-$ARCH.dmg
export PATH="$HOME/.cargo/bin:$PATH"

SWIFT_ARCHS=()
NEKOD=target/release/nekod
if [ -n "${UNIVERSAL:-}" ]; then
  SWIFT_ARCHS=(--arch arm64 --arch x86_64)
  NEKOD=target/universal/nekod
fi

if [ -z "${NEKO_SKIP_BUILD:-}" ]; then
  echo "==> building nekod"
  if [ -n "${UNIVERSAL:-}" ]; then
    for t in aarch64-apple-darwin x86_64-apple-darwin; do
      rustup target list --installed | grep -qx "$t" || { echo "missing Rust target $t: run rustup target add $t" >&2; exit 1; }
      cargo build --release -p neko-cli --target "$t"
    done
    mkdir -p target/universal
    lipo -create target/aarch64-apple-darwin/release/nekod target/x86_64-apple-darwin/release/nekod -output "$NEKOD"
  else
    cargo build --release -p neko-cli
  fi
  echo "==> building the app"
  (cd desktop-macos && swift build -c release --product Neko ${SWIFT_ARCHS[@]+"${SWIFT_ARCHS[@]}"})
fi
SWIFT_BIN=$(cd desktop-macos && swift build -c release --show-bin-path ${SWIFT_ARCHS[@]+"${SWIFT_ARCHS[@]}"})

rm -rf "$APP" "$OUT"/*.dmg "$OUT"/*.sha256 "$OUT/dmg"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m 0755 "$SWIFT_BIN/Neko" "$APP/Contents/MacOS/Neko"
install -m 0755 "$NEKOD" "$APP/Contents/MacOS/nekod"
cp -R desktop-macos/Sources/NekoApp/Resources/Fonts "$APP/Contents/Resources/Fonts"
sed "s/@VERSION@/$VERSION/g" packaging/macos/Info.plist.in > "$APP/Contents/Info.plist"

echo "==> icon"
ICONSET=$OUT/Neko.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s packaging/icon/neko-1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) packaging/icon/neko-1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
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
codesign --verify --strict --deep "$APP"
[ -n "${UNIVERSAL:-}" ] && lipo -archs "$APP/Contents/MacOS/Neko" "$APP/Contents/MacOS/nekod"

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
  xcrun stapler validate "$DMG"
  spctl --assess --type open --context context:primary-signature -v "$DMG"
fi

(cd "$OUT" && shasum -a 256 "$(basename "$DMG")" > "$(basename "$DMG").sha256")
echo
echo "built $DMG"
[ -z "${SIGN_IDENTITY:-}" ] && echo "ad-hoc signed: on another Mac, right-click Neko.app > Open the first time (or run: xattr -dr com.apple.quarantine /Applications/Neko.app)"
exit 0
