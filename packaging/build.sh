#!/usr/bin/env bash
# Build the portable tarball and a .deb for the current Linux architecture.
# No dpkg needed: the .deb is assembled with ar and tar.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
case "$(uname -m)" in
  x86_64)  ARCH=x86_64;  DEB_ARCH=amd64 ;;
  aarch64) ARCH=aarch64; DEB_ARCH=arm64 ;;
  *) echo "unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac
NAME=proxy-$VERSION-linux-$ARCH
OUT=$ROOT/dist
STAGE=$OUT/$NAME

echo "==> building release binaries"
cargo build --release -p proxy-cli -p proxy-desktop

rm -rf "$OUT"
mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/icons/hicolor/scalable/apps"
install -m 0755 target/release/proxy  "$STAGE/bin/proxy"
install -m 0755 target/release/proxyd "$STAGE/bin/proxyd"
install -m 0644 packaging/dev.proxy.Proxy.desktop "$STAGE/share/applications/"
install -m 0644 packaging/dev.proxy.Proxy.svg     "$STAGE/share/icons/hicolor/scalable/apps/"
cat > "$STAGE/README.txt" <<TXT
Proxy $VERSION

  bin/proxy    the desktop app (starts and supervises proxyd)
  bin/proxyd   the capture core, also usable headless: proxyd run | flows | export --har | ca path

Run ./bin/proxy. On first launch open Settings > Certificate to trust the proxy CA.
TXT

echo "==> tarball"
tar -C "$OUT" --owner=0 --group=0 -czf "$OUT/$NAME.tar.gz" "$NAME"

echo "==> deb"
DEB=$OUT/deb
mkdir -p "$DEB/data/usr/bin" "$DEB/data/usr/share/applications" \
         "$DEB/data/usr/share/icons/hicolor/scalable/apps" "$DEB/data/usr/share/doc/proxy" "$DEB/control"
install -m 0755 target/release/proxy  "$DEB/data/usr/bin/proxy"
install -m 0755 target/release/proxyd "$DEB/data/usr/bin/proxyd"
install -m 0644 packaging/dev.proxy.Proxy.desktop "$DEB/data/usr/share/applications/"
install -m 0644 packaging/dev.proxy.Proxy.svg     "$DEB/data/usr/share/icons/hicolor/scalable/apps/"
cat > "$DEB/data/usr/share/doc/proxy/copyright" <<TXT
Proxy $VERSION
License: MIT OR Apache-2.0 (declared in Cargo.toml; confirm before release)
TXT
INSTALLED_KB=$(du -sk "$DEB/data" | cut -f1)
cat > "$DEB/control/control" <<TXT
Package: proxy
Version: $VERSION
Section: devel
Priority: optional
Architecture: $DEB_ARCH
Installed-Size: $INSTALLED_KB
Depends: libc6 (>= 2.31), libgl1 | libegl1, libxkbcommon0, libwayland-client0 | libx11-6
Recommends: libnss3-tools, xdg-utils, policykit-1
Maintainer: Proxy contributors <noreply@example.invalid>
Description: HTTP and HTTPS debugging proxy
 Capture, inspect and search HTTP/1.1 and HTTP/2 traffic, decrypt HTTPS with a
 per-install certificate, and export HAR. Ships a native desktop app and the
 headless capture core (proxyd).
TXT
( cd "$DEB/data" && find . -type f -exec md5sum {} + | sed 's# \./# #' ) > "$DEB/control/md5sums"
echo "2.0" > "$DEB/debian-binary"
tar -C "$DEB/control" --owner=0 --group=0 -czf "$DEB/control.tar.gz" .
tar -C "$DEB/data"    --owner=0 --group=0 -czf "$DEB/data.tar.gz" .
( cd "$DEB" && ar rc "$OUT/proxy_${VERSION}_${DEB_ARCH}.deb" debian-binary control.tar.gz data.tar.gz )
rm -rf "$DEB"

echo "==> done"
ls -lh "$OUT"/*.tar.gz "$OUT"/*.deb
