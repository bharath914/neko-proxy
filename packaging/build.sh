#!/usr/bin/env bash
# Build release packages for the current Linux architecture into dist/:
#   proxy-<ver>-linux-<arch>.tar.gz    portable tarball (also what install.sh downloads)
#   proxy_<ver>_<deb-arch>.deb         Debian/Ubuntu, assembled with ar and tar (no dpkg needed)
#   proxy-<ver>-1.<arch>.rpm           Fedora/RHEL/openSUSE, needs rpmbuild (skipped if absent)
#   Proxy-<ver>-<arch>.AppImage        needs APPIMAGETOOL=/path/to/appimagetool (skipped if unset)
#   SHA256SUMS                         checksums of everything above
#
# Build on the oldest distro you want to support: the glibc floor of the binaries is
# written into the package dependencies and the release workflow builds on Ubuntu 22.04.
#
# Env: PROXY_SKIP_BUILD=1 reuses target/release, PROXY_MAINTAINER sets the .deb maintainer.
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
MAINTAINER=${PROXY_MAINTAINER:-Proxy contributors <noreply@example.invalid>}

if [ -z "${PROXY_SKIP_BUILD:-}" ]; then
  echo "==> building release binaries"
  cargo build --release -p proxy-cli -p proxy-desktop
fi

# Highest glibc symbol version the binaries need. Tells users what distros can run them.
GLIBC=$(objdump -T target/release/proxy target/release/proxyd \
  | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1)
echo "==> glibc floor: ${GLIBC:-none}"

rm -rf "$OUT"
mkdir -p "$STAGE/bin" "$STAGE/share/applications" "$STAGE/share/icons/hicolor/scalable/apps"
install -m 0755 target/release/proxy  "$STAGE/bin/proxy"
install -m 0755 target/release/proxyd "$STAGE/bin/proxyd"
install -m 0644 packaging/dev.proxy.Proxy.desktop "$STAGE/share/applications/"
install -m 0644 packaging/dev.proxy.Proxy.svg     "$STAGE/share/icons/hicolor/scalable/apps/"
cat > "$STAGE/README.txt" <<TXT
Proxy $VERSION (needs glibc ${GLIBC:-any})

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
Depends: libc6 (>= ${GLIBC:-2.31}), libgl1 | libegl1, libxkbcommon0, libwayland-client0 | libx11-6
Recommends: libnss3-tools, xdg-utils, policykit-1
Maintainer: $MAINTAINER
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

if command -v rpmbuild >/dev/null; then
  echo "==> rpm"
  RPM=$OUT/rpmtop
  mkdir -p "$RPM"
  cat > "$RPM/proxy.spec" <<TXT
Name:           proxy
Version:        $VERSION
Release:        1
Summary:        HTTP and HTTPS debugging proxy
License:        MIT OR Apache-2.0
BuildArch:      $ARCH
Recommends:     nss-tools
Recommends:     xdg-utils

%global debug_package %{nil}
%global __os_install_post %{nil}

%description
Capture, inspect and search HTTP/1.1 and HTTP/2 traffic, decrypt HTTPS with a
per-install certificate, and export HAR. Ships a native desktop app (proxy) and
the headless capture core (proxyd).

%install
install -Dm755 $STAGE/bin/proxy  %{buildroot}%{_bindir}/proxy
install -Dm755 $STAGE/bin/proxyd %{buildroot}%{_bindir}/proxyd
install -Dm644 $STAGE/share/applications/dev.proxy.Proxy.desktop %{buildroot}%{_datadir}/applications/dev.proxy.Proxy.desktop
install -Dm644 $STAGE/share/icons/hicolor/scalable/apps/dev.proxy.Proxy.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/dev.proxy.Proxy.svg

%files
%{_bindir}/proxy
%{_bindir}/proxyd
%{_datadir}/applications/dev.proxy.Proxy.desktop
%{_datadir}/icons/hicolor/scalable/apps/dev.proxy.Proxy.svg
TXT
  rpmbuild -bb --quiet --define "_topdir $RPM" --define "_rpmdir $RPM/out" "$RPM/proxy.spec"
  mv "$RPM"/out/*/*.rpm "$OUT/"
  rm -rf "$RPM"
else
  echo "==> rpm skipped (rpmbuild not found)"
fi

if [ -n "${APPIMAGETOOL:-}" ]; then
  echo "==> appimage"
  APPDIR=$OUT/Proxy.AppDir
  mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/scalable/apps"
  install -m 0755 target/release/proxy  "$APPDIR/usr/bin/proxy"
  install -m 0755 target/release/proxyd "$APPDIR/usr/bin/proxyd"
  install -m 0644 packaging/dev.proxy.Proxy.desktop "$APPDIR/usr/share/applications/"
  install -m 0644 packaging/dev.proxy.Proxy.svg     "$APPDIR/usr/share/icons/hicolor/scalable/apps/"
  install -m 0644 packaging/dev.proxy.Proxy.desktop "$APPDIR/"
  install -m 0644 packaging/dev.proxy.Proxy.svg     "$APPDIR/"
  # proxy finds proxyd next to its own executable, so only the PATH needs help.
  cat > "$APPDIR/AppRun" <<'SH'
#!/bin/sh
HERE=$(dirname "$(readlink -f "$0")")
export PATH="$HERE/usr/bin:$PATH"
exec "$HERE/usr/bin/proxy" "$@"
SH
  chmod 0755 "$APPDIR/AppRun"
  # Extract-and-run so it works on build machines without FUSE.
  ARCH=$ARCH APPIMAGE_EXTRACT_AND_RUN=1 "$APPIMAGETOOL" --no-appstream "$APPDIR" \
    "$OUT/Proxy-$VERSION-$ARCH.AppImage"
  rm -rf "$APPDIR"
else
  echo "==> appimage skipped (set APPIMAGETOOL=/path/to/appimagetool)"
fi

echo "==> checksums"
( cd "$OUT" && shopt -s nullglob && sha256sum *.tar.gz *.deb *.rpm *.AppImage > SHA256SUMS )

echo "==> done"
( cd "$OUT" && shopt -s nullglob && ls -lh *.tar.gz *.deb *.rpm *.AppImage )
