"""Archive/layout tests with inert ELF fixtures, never release executables."""
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import struct
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("neko_linux_build", Path(__file__).with_name("build.py"))
build = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(build)


def fixture_elf(machine=62):
    data = bytearray(64)
    data[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<HH", data, 16, 3, machine)
    return bytes(data) + b"GLIBC_2.34\x00"


def ar_members(data):
    assert data[:8] == b"!<arch>\n"
    offset = 8
    members = {}
    while offset < len(data):
        header = data[offset:offset+60]
        assert header[58:] == b"`\n"
        length = int(header[48:58])
        name = header[:16].decode().strip().removesuffix("/")
        members[name] = data[offset+60:offset+60+length]
        offset += 60 + length + length % 2
    return members


class Packaging(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="neko-packaging-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.binaries = self.root / "binaries"
        self.binaries.mkdir()
        for name in ("neko", "nekod"):
            (self.binaries / name).write_bytes(fixture_elf())
        self.output = self.root / "dist"
        self.args = ["--skip-build", "--arch", "x86_64", "--binary-dir", str(self.binaries), "--out-dir", str(self.output)]

    def test_architecture_validation(self):
        self.assertEqual(build.elf_floor(self.binaries / "neko", "x86_64"), (2, 34))
        with self.assertRaises(ValueError):
            build.elf_floor(self.binaries / "neko", "aarch64")
        (self.binaries / "neko").write_bytes(b"Mach-O, not Linux")
        with self.assertRaises(ValueError):
            build.main(self.args + ["--formats", "deb"])

    def test_deb_tar_appdir_reproducible_and_preserve_other_artifacts(self):
        self.output.mkdir()
        sentinel = self.output / "Neko-macOS.dmg"
        sentinel.write_bytes(b"do not delete macOS artifacts")
        with patch.dict(os.environ, {"SOURCE_DATE_EPOCH": "0"}):
            artifacts = build.main(self.args + ["--formats", "tar,deb,appdir"])
            first = {p.name: p.read_bytes() for p in artifacts}
            build.main(self.args + ["--formats", "tar,deb,appdir"])
        self.assertEqual(first, {p.name: p.read_bytes() for p in artifacts})
        self.assertEqual(sentinel.read_bytes(), b"do not delete macOS artifacts")
        package = next(p for p in artifacts if p.suffix == ".deb")
        members = ar_members(package.read_bytes())
        if shutil.which("ar"):
            listed = subprocess.run(["ar", "-t", str(package)], capture_output=True, text=True, check=True)
            self.assertEqual([name.removesuffix("/") for name in listed.stdout.splitlines()], ["debian-binary", "control.tar.gz", "data.tar.gz"])
        self.assertEqual(list(members), ["debian-binary", "control.tar.gz", "data.tar.gz"])
        self.assertEqual(members["debian-binary"], b"2.0\n")
        with tarfile.open(fileobj=io.BytesIO(members["control.tar.gz"])) as tar:
            control = tar.extractfile("control").read().decode()
            self.assertIn("Package: neko\n", control)
            self.assertIn("Architecture: amd64\n", control)
            self.assertIn("libc6 (>= 2.34)", control)
            md5sums = tar.extractfile("md5sums").read().decode()
        with tarfile.open(fileobj=io.BytesIO(members["data.tar.gz"])) as tar:
            for member in tar.getmembers():
                self.assertEqual((member.uid, member.gid), (0, 0))
                self.assertFalse(member.name.startswith("/"))
                self.assertNotIn("..", Path(member.name).parts)
            self.assertEqual(tar.getmember("usr/bin/neko").mode, 0o755)
            self.assertEqual(tar.extractfile("usr/bin/nekod").read(), fixture_elf())
            desktop = tar.extractfile(f"usr/share/applications/{build.APP_ID}.desktop").read().decode()
            self.assertIn("Exec=neko\n", desktop)
            self.assertIn("usr/bin/neko", md5sums)
            self.assertIn("usr/share/doc/neko/LICENSE-MIT", tar.getnames())
            self.assertIn("usr/share/doc/neko/LICENSE-APACHE", tar.getnames())
            self.assertFalse(any("ca.key" in name or "mcp.token" in name for name in tar.getnames()))
        tree = self.output / "Neko.AppDir"
        self.assertTrue((tree / "usr/bin/nekod").is_file())
        self.assertTrue((tree / ".DirIcon").is_symlink())
        subprocess.run(["sh", "-n", str(tree / "AppRun")], check=True)
        # A stand-in verifies argument forwarding only; it is never packaged.
        (tree / "usr/bin/neko").write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n')
        result = subprocess.run([str(tree / "AppRun"), "import-har", "with spaces.har"], capture_output=True, text=True, check=True)
        self.assertEqual(result.stdout.splitlines(), ["import-har", "with spaces.har"])
        sums = (self.output / "SHA256SUMS-linux-x86_64").read_text()
        self.assertIn(hashlib.sha256(package.read_bytes()).hexdigest(), sums)

    def test_offline_installer_and_uninstaller(self):
        tarball = build.main(self.args + ["--formats", "tar"])[0]
        tools = self.root / "tools"
        tools.mkdir()
        uname = tools / "uname"
        uname.write_text('#!/bin/sh\ncase "$1" in -s) echo Linux;; -m) echo x86_64;; *) /usr/bin/uname "$@";; esac\n')
        uname.chmod(0o755)
        prefix = self.root / "install with spaces & punctuation"
        environment = {**os.environ, "PATH": str(tools) + os.pathsep + os.environ["PATH"]}
        script = build.ROOT / "packaging/install.sh"
        subprocess.run(["sh", str(script), "--tarball", str(tarball), "--prefix", str(prefix)], env=environment, check=True, capture_output=True)
        self.assertEqual((prefix / "bin/neko").read_bytes(), fixture_elf())
        self.assertTrue((prefix / "bin/nekod").exists())
        desktop = (prefix / f"share/applications/{build.APP_ID}.desktop").read_text()
        self.assertIn(f'Exec="{prefix}/bin/neko"', desktop)
        subprocess.run(["sh", str(script), "--uninstall", "--prefix", str(prefix)], env=environment, check=True, capture_output=True)
        self.assertFalse((prefix / "bin/neko").exists())
        self.assertFalse((prefix / "bin/nekod").exists())

    def test_arm64_control(self):
        for name in ("neko", "nekod"):
            (self.binaries / name).write_bytes(fixture_elf(183))
        args = self.args.copy()
        args[args.index("x86_64")] = "aarch64"
        package = build.main(args + ["--formats", "deb"])[0]
        control = ar_members(package.read_bytes())["control.tar.gz"]
        with tarfile.open(fileobj=io.BytesIO(control)) as tar:
            self.assertIn("Architecture: arm64\n", tar.extractfile("control").read().decode())

    def test_appimagetool_contract_with_explicit_test_double(self):
        tool = self.root / "appimagetool-test-double"
        # This verifies orchestration, NOT the real AppImage runtime or format.
        tool.write_text('''#!/usr/bin/env python3
import os, pathlib, sys
source, target = map(pathlib.Path, sys.argv[-2:])
assert sys.argv[1] == '--no-appstream'
assert os.environ['ARCH'] == 'x86_64'
assert os.environ['APPIMAGE_EXTRACT_AND_RUN'] == '1'
assert (source/'AppRun').is_file()
assert 'Exec=neko' in next(source.glob('*.desktop')).read_text()
assert (source/'usr/bin/nekod').is_file()
target.write_bytes(b'EXPLICIT APPIMAGE TEST DOUBLE - NOT A RELEASE')
''')
        tool.chmod(0o755)
        package = build.main(self.args + ["--formats", "appimage", "--appimagetool", str(tool)])[0]
        self.assertTrue(package.stat().st_mode & 0o111)
        self.assertIn(b"TEST DOUBLE", package.read_bytes())

    def test_appimage_prerequisite_is_explicit(self):
        with patch.dict(os.environ, {"APPIMAGETOOL": ""}):
            with self.assertRaises(SystemExit):
                build.main(self.args + ["--formats", "appimage"])


if __name__ == "__main__":
    unittest.main()
