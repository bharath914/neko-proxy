#!/usr/bin/env python3
"""Build Linux packages without touching other dist/ artifacts (Python 3.10+)."""
from __future__ import annotations
import argparse
import gzip
import hashlib
import io
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import struct
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
ARCHES = {"x86_64": (62, "amd64"), "aarch64": (183, "arm64")}
APP_ID = "dev.nekoproxy.Neko"
APP_RUN = '''#!/bin/sh
set -eu
if [ -z "${APPDIR:-}" ]; then APPDIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd); fi
export PATH="$APPDIR/usr/bin:$PATH"
exec "$APPDIR/usr/bin/neko" "$@"
'''


def version() -> str:
    value = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)
    if not value or not re.fullmatch(r"[0-9][0-9A-Za-z.+~-]*", value[1]):
        raise ValueError("invalid workspace version")
    return value[1]


def elf_floor(binary: Path, arch: str) -> tuple[int, ...]:
    data = binary.read_bytes()
    if len(data) < 64 or data[:6] != b"\x7fELF\x02\x01":
        raise ValueError(f"{binary} must be a 64-bit little-endian Linux ELF binary")
    kind, machine = struct.unpack_from("<HH", data, 16)
    if kind not in (2, 3) or machine != ARCHES[arch][0]:
        raise ValueError(f"{binary} is not an executable for {arch}")
    versions = [tuple(map(int, match.split(b"."))) for match in re.findall(rb"\bGLIBC_([0-9]+(?:\.[0-9]+)+)\x00", data)]
    return max(versions, default=(2, 31))


def copy(source: Path, destination: Path, mode: int = 0o644) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(mode)


def stage(binary_dir: Path, target: Path, ver: str, floor: str) -> None:
    for binary in ("neko", "nekod"):
        copy(binary_dir / binary, target / "bin" / binary, 0o755)
    copy(ROOT / "packaging" / f"{APP_ID}.desktop", target / f"share/applications/{APP_ID}.desktop")
    copy(ROOT / "packaging/icon/neko-512.png", target / f"share/icons/hicolor/512x512/apps/{APP_ID}.png")
    doc = target / "share/doc/neko"
    for license in ("LICENSE-MIT", "LICENSE-APACHE"):
        copy(ROOT / license, doc / license)
    (doc / "copyright").write_text(f"Neko Proxy {ver}\nLicense: MIT OR Apache-2.0\nSee LICENSE-MIT and LICENSE-APACHE.\n")
    (target / "README.txt").write_text(
        f"Neko Proxy {ver} (Linux; glibc >= {floor})\n\n"
        "bin/neko starts the desktop app and supervises bin/nekod.\n"
        "bin/nekod run is the headless proxy. HTTP and SOCKS5: 127.0.0.1:8899.\n"
        "neko import-har <file> and neko export --kind <kind> use the core CLI.\n"
        "Open Settings > Certificate to opt into trusting the local CA.\n"
        "The package never changes your proxy or certificate trust settings.\n"
    )

    (doc / "copyright").chmod(0o644)
    (target / "README.txt").chmod(0o644)
    target.chmod(0o755)
    for directory in target.rglob("*"):
        if directory.is_dir():
            directory.chmod(0o755)


def archive(root: Path, prefix: str = "", epoch: int = 0) -> bytes:
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.USTAR_FORMAT) as tar:
        paths = [root] + sorted(root.rglob("*")) if prefix else sorted(root.rglob("*"))
        for path in paths:
            relative = str(path.relative_to(root))
            name = str(Path(prefix) / relative) if prefix else relative
            info = tar.gettarinfo(str(path), arcname=name)
            info.uid = info.gid = 0
            info.uname = info.gname = "root"
            info.mtime = epoch
            if info.isdir():
                info.mode = 0o755
            elif info.isfile():
                info.mode = 0o755 if info.mode & 0o111 else 0o644
            if info.isfile():
                with path.open("rb") as source:
                    tar.addfile(info, source)
            else:
                tar.addfile(info)
    return gzip.compress(raw.getvalue(), mtime=epoch)


def atomic_write(path: Path, data: bytes) -> None:
    fd, name = tempfile.mkstemp(prefix=".neko-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as output:
            output.write(data)
        os.chmod(name, 0o644)
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def deb(stage_dir: Path, destination: Path, ver: str, arch: str, floor: str, maintainer: str, epoch: int) -> None:
    with tempfile.TemporaryDirectory(prefix="neko-deb-") as temp:
        root = Path(temp)
        data = root / "data"
        shutil.copytree(stage_dir / "bin", data / "usr/bin")
        shutil.copytree(stage_dir / "share", data / "usr/share")
        control = root / "control"
        control.mkdir()
        size = sum(p.stat().st_size for p in data.rglob("*") if p.is_file())
        (control / "control").write_text(
            f"Package: neko\nVersion: {ver}\nSection: devel\nPriority: optional\nArchitecture: {ARCHES[arch][1]}\n"
            f"Installed-Size: {(size + 1023) // 1024}\n"
            f"Depends: libc6 (>= {floor}), libgcc-s1, libgl1 | libegl1, libxkbcommon0, libwayland-client0 | libx11-6\n"
            "Recommends: ca-certificates, libnss3-tools, xdg-utils, policykit-1\n"
            f"Maintainer: {maintainer}\nDescription: Neko HTTP and HTTPS debugging proxy\n"
            " Native desktop inspector and headless core with HTTP/SOCKS5 capture,\n"
            " TLS interception, rules, SDK ingest and redacted traffic exports.\n"
        )
        (control / "md5sums").write_text("".join(
            f"{hashlib.md5(p.read_bytes(), usedforsecurity=False).hexdigest()}  {p.relative_to(data)}\n"
            for p in sorted(data.rglob("*")) if p.is_file()
        ))
        members = [("debian-binary", b"2.0\n"), ("control.tar.gz", archive(control, epoch=epoch)), ("data.tar.gz", archive(data, epoch=epoch))]
        output = io.BytesIO(b"!<arch>\n")
        output.seek(8)
        for name, content in members:
            header = f"{name + '/':<16}{epoch:<12}{0:<6}{0:<6}{'100644':<8}{len(content):<10}`\n"
            output.write(header.encode("ascii"))
            output.write(content)
            if len(content) % 2:
                output.write(b"\n")
        atomic_write(destination, output.getvalue())


def appdir(stage_dir: Path, target: Path) -> None:
    (target / "usr").mkdir(parents=True)
    shutil.copytree(stage_dir / "bin", target / "usr/bin")
    shutil.copytree(stage_dir / "share", target / "usr/share")
    copy(ROOT / "packaging" / f"{APP_ID}.desktop", target / f"{APP_ID}.desktop")
    copy(ROOT / "packaging/icon/neko-512.png", target / f"{APP_ID}.png")
    (target / ".DirIcon").symlink_to(f"{APP_ID}.png")
    (target / "AppRun").write_text(APP_RUN)
    (target / "AppRun").chmod(0o755)
    target.chmod(0o755)
    for directory in target.rglob("*"):
        if directory.is_dir():
            directory.chmod(0o755)


def rpm(stage_dir: Path, output: Path, ver: str, arch: str, floor: str) -> list[Path]:
    with tempfile.TemporaryDirectory(prefix="neko-rpm-") as temp:
        root = Path(temp)
        spec = root / "neko.spec"
        source = shlex.quote(str(stage_dir))
        spec.write_text(f"""Name: neko
Version: {ver}
Release: 1
Summary: HTTP and HTTPS debugging proxy
License: MIT OR Apache-2.0
BuildArch: {arch}
Requires: glibc >= {floor}
%global debug_package %{{nil}}
%global __os_install_post %{{nil}}
%description
Native inspector and headless capture core.
%install
mkdir -p %{{buildroot}}/usr/bin %{{buildroot}}/usr/share
cp -a {source}/bin/. %{{buildroot}}/usr/bin/
cp -a {source}/share/. %{{buildroot}}/usr/share/
%files
/usr/bin/neko
/usr/bin/nekod
/usr/share/applications/{APP_ID}.desktop
/usr/share/icons/hicolor/512x512/apps/{APP_ID}.png
/usr/share/doc/neko
""")
        subprocess.run(["rpmbuild", "-bb", "--quiet", "--define", f"_topdir {root}", str(spec)], check=True)
        result = []
        for package in root.glob("RPMS/*/*.rpm"):
            target = output / package.name
            copy(package, target)
            result.append(target)
        return result


def main(argv: list[str] | None = None) -> list[Path]:
    parser = argparse.ArgumentParser(description=__doc__)
    native_arch = {"arm64": "aarch64", "amd64": "x86_64"}.get(platform.machine(), platform.machine())
    parser.add_argument("--arch", choices=ARCHES, default=native_arch)
    parser.add_argument("--skip-build", action="store_true", default=os.getenv("NEKO_SKIP_BUILD") == "1")
    parser.add_argument("--binary-dir", type=Path)
    parser.add_argument("--target", help="Cargo target triple (build on Linux)")
    parser.add_argument("--out-dir", type=Path, default=ROOT / "dist")
    parser.add_argument("--appimagetool", default=os.getenv("APPIMAGETOOL"))
    parser.add_argument("--runtime-file", type=Path, help="Optional offline AppImage runtime")
    parser.add_argument("--formats", help="Comma-separated tar,deb,appdir,appimage,rpm")
    args = parser.parse_args(argv)
    if args.arch not in ARCHES:
        parser.error("supported architectures: x86_64 and aarch64")
    default_formats = ["tar", "deb"] + (["rpm"] if shutil.which("rpmbuild") else []) + (["appimage"] if args.appimagetool else [])
    formats = set(args.formats.split(",") if args.formats else default_formats)
    if not formats or formats - {"tar", "deb", "appdir", "appimage", "rpm"}:
        parser.error("unknown or empty format list")
    if "appimage" in formats and not args.appimagetool:
        parser.error("AppImage requires --appimagetool or APPIMAGETOOL")
    if not args.skip_build:
        if platform.system() != "Linux":
            parser.error("build Linux binaries on Linux; --skip-build accepts prebuilt Linux ELF files")
        command = ["cargo", "build", "--release", "-p", "neko-cli", "-p", "neko-desktop"]
        if args.target:
            command += ["--target", args.target]
        subprocess.run(command, cwd=ROOT, check=True)
    target_dir = Path(os.getenv("CARGO_TARGET_DIR", str(ROOT / "target")))
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    binaries = args.binary_dir or (target_dir / args.target / "release" if args.target else target_dir / "release")
    floor = ".".join(map(str, max(elf_floor(binaries / name, args.arch) for name in ("neko", "nekod"))))
    ver = version()
    epoch = int(os.getenv("SOURCE_DATE_EPOCH", "0"))
    if not 0 <= epoch <= 0xffffffff:
        parser.error("SOURCE_DATE_EPOCH must fit an unsigned 32-bit timestamp")
    maintainer = os.getenv("NEKO_MAINTAINER", "Neko Proxy contributors <noreply@example.invalid>")
    if "\n" in maintainer or "\r" in maintainer:
        parser.error("maintainer must be a single line")
    output = args.out_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    artifacts = []
    with tempfile.TemporaryDirectory(prefix=".neko-stage-", dir=output) as temp:
        source = Path(temp) / f"neko-{ver}-linux-{args.arch}"
        stage(binaries, source, ver, floor)
        if "tar" in formats:
            target = output / f"{source.name}.tar.gz"
            atomic_write(target, archive(source, source.name, epoch))
            artifacts.append(target)
        if "deb" in formats:
            target = output / f"neko_{ver}_{ARCHES[args.arch][1]}.deb"
            deb(source, target, ver, args.arch, floor, maintainer, epoch)
            artifacts.append(target)
        if "appdir" in formats or "appimage" in formats:
            tree = Path(temp) / "Neko.AppDir"
            appdir(source, tree)
            if "appdir" in formats:
                destination = output / "Neko.AppDir"
                if destination.is_symlink():
                    destination.unlink()
                elif destination.exists():
                    shutil.rmtree(destination)
                shutil.copytree(tree, destination, symlinks=True)
            if "appimage" in formats:
                target = output / f"Neko-{ver}-{args.arch}.AppImage"
                temporary_image = Path(temp) / target.name
                tool = shutil.which(args.appimagetool) or str(Path(args.appimagetool).resolve())
                command = [tool, "--no-appstream"]
                if args.runtime_file:
                    command += ["--runtime-file", str(args.runtime_file.resolve())]
                command += [str(tree), str(temporary_image)]
                env = {**os.environ, "ARCH": args.arch, "APPIMAGE_EXTRACT_AND_RUN": "1", "SOURCE_DATE_EPOCH": str(epoch)}
                subprocess.run(command, env=env, check=True)
                if not temporary_image.is_file():
                    raise ValueError("appimagetool returned without producing an AppImage")
                os.replace(temporary_image, target)
                target.chmod(0o755)
                artifacts.append(target)
        if "rpm" in formats:
            artifacts += rpm(source, output, ver, args.arch, floor)
    # Separate Linux checksums preserve any existing macOS release checksums.
    sums = "".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n" for path in sorted(artifacts))
    atomic_write(output / f"SHA256SUMS-linux-{args.arch}", sums.encode())
    # install.sh consumes this conventional filename; include all existing
    # release artifacts without deleting or modifying any of them.
    files = sorted({path for pattern in ("*.tar.gz", "*.deb", "*.rpm", "*.AppImage", "*.dmg", "*.zip") for path in output.glob(pattern) if path.is_file()})
    atomic_write(output / "SHA256SUMS", "".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n" for path in files).encode())
    print(f"Linux {args.arch}; glibc >= {floor}")
    for artifact in artifacts:
        print(artifact)
    return artifacts


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"packaging: {error}")
