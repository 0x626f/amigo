"""Integrity failures must be caught before any native installer is executed."""

import io
import json
from pathlib import Path
import plistlib
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import distribute as dist
import validate_packages as validate


class PackageValidationTests(unittest.TestCase):
    def test_waits_for_asynchronous_uninstaller_cleanup(self):
        with tempfile.TemporaryDirectory(prefix="amigo-uninstall-test-") as temp:
            app = Path(temp) / "Amigo"
            app.mkdir()
            (app / "unins000.exe").write_bytes(b"still being cleaned up")
            child = subprocess.Popen([sys.executable, "-c",
                                      "import shutil,sys,time;time.sleep(.15);shutil.rmtree(sys.argv[1])", str(app)])
            try:
                validate.wait_removed(app, timeout=10)
            finally:
                child.wait(timeout=10)
            self.assertEqual(child.returncode, 0)
            self.assertFalse(app.exists())

    @unittest.skipUnless(shutil.which("ar") and shutil.which("zstd"), "requires Linux DEB inspection tools")
    def test_zstd_deb_payload_is_readable_before_python_314(self):
        with tempfile.TemporaryDirectory(prefix="amigo-zstd-deb-") as temp:
            root = Path(temp)
            raw = io.BytesIO()
            with tarfile.open(fileobj=raw, mode="w") as archive:
                member = tarfile.TarInfo("payload")
                member.size = 4
                archive.addfile(member, io.BytesIO(b"note"))
            compressed = subprocess.run(["zstd", "--compress", "--stdout"], input=raw.getvalue(),
                                        stdout=subprocess.PIPE, check=True).stdout
            (root / "data.tar.zst").write_bytes(compressed)
            subprocess.run(["ar", "rc", str(root / "test.deb"), str(root / "data.tar.zst")], check=True)
            with tarfile.open(fileobj=io.BytesIO(validate.deb_part(root / "test.deb", "data.tar"))) as archive:
                self.assertEqual(archive.extractfile("payload").read(), b"note")

    def test_elf_comparison_ignores_stripped_symbols_but_detects_changed_code(self):
        binary = bytearray(256)
        binary[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<Q", binary, 40, 64)
        struct.pack_into("<HH", binary, 58, 64, 2)
        struct.pack_into("<IQQQQ", binary, 68, 1, 2, 0x1000, 192, 4)
        struct.pack_into("<IQQQQ", binary, 132, 2, 0, 0, 196, 4)
        binary[192:200] = b"codeSYMS"
        stripped = bytearray(binary)
        stripped[196:200] = b"none"
        self.assertEqual(validate.elf_sections(binary), validate.elf_sections(stripped))
        stripped[192:196] = b"evil"
        self.assertNotEqual(validate.elf_sections(binary), validate.elf_sections(stripped))

    def fixture(self, platform="windows", arch="x86_64"):
        temp = tempfile.TemporaryDirectory(prefix="amigo-validation-test-")
        self.addCleanup(temp.cleanup)
        root = Path(temp.name)
        stem = f"amigo-{dist.version()}-{platform}-{arch}"
        binary = bytearray(128)
        if platform == "windows":
            binary[:2] = b"MZ"
            struct.pack_into("<I", binary, 60, 64)
            binary[64:68] = b"PE\0\0"
            struct.pack_into("<H", binary, 68, 0x8664 if arch == "x86_64" else 0xaa64)
            suffix, executable, docs = ".zip", "Amigo/amigo.exe", "Amigo/docs/"
        else:
            binary[:4] = b"\xcf\xfa\xed\xfe"
            struct.pack_into("<I", binary, 4, 0x1000007 if arch == "x86_64" else 0x100000c)
            suffix, executable, docs = ".app.zip", "Amigo.app/Contents/MacOS/amigo", "Amigo.app/Contents/Resources/docs/"
        with zipfile.ZipFile(root / (stem + suffix), "w") as archive:
            archive.writestr(executable, binary)
            for source, name in dist.documentation():
                archive.writestr(docs + name, source.read_bytes())
            if platform == "macos":
                archive.writestr("Amigo.app/Contents/Info.plist", plistlib.dumps({
                    "CFBundleShortVersionString": dist.version(), "CFBundleVersion": dist.version(),
                    "CFBundleIdentifier": "io.github.0x626f.amigo"}))
        (root / (stem + (".setup.exe" if platform == "windows" else ".dmg"))).write_bytes(b"container fixture")
        state = {"version": dist.version(), "platform": platform, "architecture": arch,
                 "commit": "release-commit", "clean": True}
        (root / f"SOURCE-{platform}-{arch}.json").write_text(json.dumps(state))
        self.checksums(root, platform, arch)
        return root, stem

    def checksums(self, root, platform, arch):
        paths = sorted(path for path in root.iterdir() if not path.name.startswith("SHA256SUMS"))
        (root / f"SHA256SUMS-{platform}-{arch}").write_text(
            "".join(f"{dist.digest(path)}  {path.name}\n" for path in paths))

    def test_zip_contents_and_architectures(self):
        for platform in ("macos", "windows"):
            for arch in ("x86_64", "aarch64"):
                with self.subTest(platform=platform, arch=arch):
                    root, _ = self.fixture(platform, arch)
                    self.assertEqual(len(validate.validate(root, "release-commit")), 4)

    def test_changed_asset_rejected_before_native_install(self):
        root, stem = self.fixture()
        (root / (stem + ".setup.exe")).write_bytes(b"tampered")
        with patch.object(dist, "system", return_value="windows"), patch.object(dist, "arch", return_value="x86_64"), \
                patch.object(validate, "windows_native") as installer:
            with self.assertRaisesRegex(ValueError, "Checksum/size mismatch"):
                validate.validate(root, "release-commit", native=True)
            installer.assert_not_called()

    def test_stale_source_rejected(self):
        root, _ = self.fixture()
        with self.assertRaisesRegex(ValueError, "source commit"):
            validate.validate(root, "another-commit")

    def test_foreign_executable_rejected_even_with_valid_hash(self):
        root, stem = self.fixture()
        path = root / (stem + ".zip")
        with zipfile.ZipFile(path) as archive:
            entries = {name: archive.read(name) for name in archive.namelist()}
        entries["Amigo/amigo.exe"] = b"incorrect executable"
        with zipfile.ZipFile(path, "w") as archive:
            for name, data in entries.items():
                archive.writestr(name, data)
        self.checksums(root, "windows", "x86_64")
        with self.assertRaisesRegex(ValueError, "architecture mismatch"):
            validate.validate(root, "release-commit")


if __name__ == "__main__":
    unittest.main()
