# Linux packages

On Linux (Python 3.10+, Rust, native linker):

```sh
APPIMAGETOOL=/path/to/appimagetool packaging/build.sh --formats tar,deb,appimage
```

Build on Ubuntu 22.04 or the oldest supported distro to set the glibc floor.
`ARCH` and `APPIMAGE_EXTRACT_AND_RUN=1` are supplied to appimagetool, so the
build host does not require FUSE. Optional `--runtime-file /path/runtime`
supports an offline AppImage runtime. appimagetool is supplied by the builder;
this script does not download or execute an unverified tool automatically.

Outputs in dist/: neko-<version>-linux-<arch>.tar.gz,
neko_<version>_<amd64|arm64>.deb, Neko-<version>-<arch>.AppImage, SHA256SUMS and
SHA256SUMS-linux-<arch>. `--formats appdir` stages Neko.AppDir for inspection.
The default builds tar/deb, plus RPM when rpmbuild is installed and AppImage
when APPIMAGETOOL is supplied. Explicit AppImage requests fail without the tool.
Existing macOS and other release artifacts are preserved.

`NEKO_SKIP_BUILD=1` or --skip-build reuses binaries; --binary-dir selects a
prebuilt directory. --arch supports x86_64 and aarch64. --target selects the
Cargo Linux target; --out-dir selects an output folder. Binaries must have the
matching 64-bit little-endian ELF executable header. Mach-O/wrong-arch inputs
are rejected. The glibc dependency uses the highest GLIBC version string in the
binaries, conservatively falling back to 2.31 for static inputs.

Debian package name and desktop Exec are now neko. Both neko and nekod, icons,
.desktop metadata and both license texts are included. Packages never install
CA keys or alter proxy/certificate settings. The portable installer uses the
same neko filenames, quotes its launcher path and supports prefixes with spaces.
Uninstall preserves capture data. RPM remains optional.

Tar/deb timestamps and ownership are normalized; SOURCE_DATE_EPOCH defaults to
0. Debian member order follows deb(5); payload checksums and installed-size are
included. AppDir's AppRun forwards arguments and keeps nekod beside neko.

Validation:
- python3 -m unittest discover -s packaging/linux -p 'test_*.py' -v: 6 passed.
- External ar inspection, amd64/arm64 metadata, archive contents/permissions,
  repeat-build identity, AppDir forwarding, checksums, Mach-O/arch rejection,
  preservation of macOS artifacts, offline install/uninstall passed.
- bash -n packaging/build.sh and sh -n packaging/install.sh passed.
- cargo test --workspace passed. No proto changes or desktop UI changes.

The tests use inert, explicitly labeled ELF fixtures. AppImage orchestration
uses a labeled test double; it does not verify an actual AppImage runtime.
This host is macOS with no Linux container runtime. Native packaging correctly
refused with exit 2:
`build.py: error: build Linux binaries on Linux; --skip-build accepts prebuilt Linux ELF files`.
Real Linux ELF build, dpkg install, RPM build, AppImage launch and Linux display
smoke tests remain for the existing Linux release workflow. No release package
was fabricated or published here.

One initial test failed because BSD ar lists optional trailing slashes in member
names: `['debian-binary/', 'control.tar.gz/', 'data.tar.gz/']`. The independent
inspection test now normalizes that allowed syntax; all tests pass.

References:
- https://manpages.debian.org/bookworm/dpkg-dev/deb.5.en.html
- https://github.com/AppImage/appimagetool
