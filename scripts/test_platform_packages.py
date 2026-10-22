"""Exercise real package contents and release metadata with synthetic native binaries."""

import contextlib
import io
import json
import os
from pathlib import Path
import plistlib
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import distribute as dist


class PlatformPackages(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory(prefix="amigo-native-package-")
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.enterContext(patch.object(dist, "ROOT", self.root))
        self.enterContext(patch.dict(os.environ, {}, clear=True))
        self.enterContext(contextlib.redirect_stdout(io.StringIO()))
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "1.2.3"\nlicense = "GPL-3.0-or-later"\n')
        files = ["README.md", "LICENSE", "docs/reference-data.md", "crates/core/COUNTRY-DATA-LICENSE",
                 "crates/core/src/countries.tsv", "crates/ui/assets/fonts/LICENSE",
                 "crates/connect/data/UNICODE-LICENSE.txt", "crates/connect/data/README.md",
                 "packaging/amigo.icns", "packaging/amigo.ico", "packaging/amigo.iss"]
        for name in files:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes("Synthetic asset — 日本語".encode())

    def binary(self, platform, architecture="x86_64"):
        data = bytearray(128)
        if platform == "linux":
            data[:6] = b"\x7fELF\x02\x01"
            struct.pack_into("<H", data, 18, 62 if architecture == "x86_64" else 183)
        elif platform == "macos":
            data[:4] = b"\xcf\xfa\xed\xfe"
            struct.pack_into("<I", data, 4, 0x1000007 if architecture == "x86_64" else 0x100000c)
            struct.pack_into("<II", data, 16, 1, 24)
            struct.pack_into("<IIIIII", data, 32, 0x32, 24, 1, 12 << 16, 14 << 16, 0)
        else:
            data[:2] = b"MZ"
            struct.pack_into("<I", data, 60, 64)
            data[64:68] = b"PE\0\0"
            struct.pack_into("<H", data, 68, 0x8664 if architecture == "x86_64" else 0xaa64)
        path = dist.binary_path()
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def test_native_architecture_aliases_and_cross_build_rejection(self):
        for machine, expected in [("AMD64", "x86_64"), ("arm64", "aarch64"), ("aarch64", "aarch64")]:
            with patch.object(dist.platform, "machine", return_value=machine):
                self.assertEqual(dist.arch(), expected)
        with (patch.dict(os.environ, {"CARGO_BUILD_TARGET": "other-target"}),
              patch.object(dist.platform, "machine", return_value="AMD64")):
            with self.assertRaisesRegex(ValueError, "Unset CARGO_BUILD_TARGET"):
                dist.arch()

    def test_foreign_package_format_is_rejected_before_any_tools_run(self):
        with patch.object(dist, "system", return_value="windows"), patch.object(dist, "run") as run:
            with self.assertRaisesRegex(ValueError, "windows supports"):
                dist.preflight(("deb",))
            run.assert_not_called()

    def test_inno_setup_is_found_in_a_per_user_installation_without_path_changes(self):
        compiler = self.root / "Programs/Inno Setup 7/ISCC.exe"
        compiler.parent.mkdir(parents=True)
        compiler.write_bytes(b"compiler")
        with patch.object(dist.shutil, "which", return_value=None), patch.dict(os.environ, {"LOCALAPPDATA": str(self.root)}):
            self.assertEqual(dist.find_tool("iscc"), str(compiler))

    def test_preflight_uses_platform_tools_and_checks_the_rust_host(self):
        cases = [("linux", "x86_64-unknown-linux-gnu", "deb"),
                 ("macos", "x86_64-apple-darwin", "dmg"),
                 ("windows", "x86_64-pc-windows-msvc", "windows")]
        for platform, host, kind in cases:
            with (self.subTest(platform=platform),
                  patch.object(dist, "system", return_value=platform),
                  patch.object(dist, "arch", return_value="x86_64"),
                  patch.object(dist, "find_tool", side_effect=lambda name: name) as lookup,
                  patch.object(dist, "output", return_value=f"host: {host}"),
                  patch.object(dist, "run"),
                  patch.dict(os.environ, {"AMIGO_MAINTAINER": "Example <me@example.com>"})):
                dist.preflight((kind,))
                requested = {call.args[0] for call in lookup.call_args_list}
                self.assertEqual("readelf" in requested, platform == "linux")
                with patch.object(dist, "output", return_value="host: wrong-toolchain"):
                    with self.assertRaisesRegex(ValueError, "native"):
                        dist.preflight((kind,))

    def test_executable_headers_reject_wrong_architecture_and_file_type(self):
        for platform in ("linux", "macos", "windows"):
            for architecture in ("x86_64", "aarch64"):
                with (self.subTest(platform=platform, architecture=architecture),
                      patch.object(dist, "system", return_value=platform),
                      patch.object(dist, "arch", return_value=architecture)):
                    binary = self.binary(platform, architecture)
                    dist.validate_binary()
                    with patch.object(dist, "arch", return_value="other"):
                        with self.assertRaisesRegex(ValueError, "native"):
                            dist.validate_binary()
                    binary.write_bytes(b"not an executable")
                    with self.assertRaisesRegex(ValueError, "native"):
                        dist.validate_binary()
                    for incomplete in (b"\x7fELF\x02", b"\xcf\xfa\xed\xfe", b"MZ"):
                        binary.write_bytes(incomplete)
                        with self.assertRaisesRegex(ValueError, "native"):
                            dist.validate_binary()

    def test_macos_deployment_target_comes_from_executable_not_packaging_environment(self):
        with patch.object(dist, "system", return_value="macos"):
            path = self.binary("macos")
            with patch.dict(os.environ, {"MACOSX_DEPLOYMENT_TARGET": "15.0"}):
                self.assertEqual(dist.macos_minimum(), "12.0.0")
            data = bytearray(path.read_bytes())
            struct.pack_into("<I", data, 40, 2)  # LC_BUILD_VERSION platform: iOS.
            path.write_bytes(data)
            with self.assertRaisesRegex(ValueError, "other than macOS"):
                dist.macos_minimum()

    def test_windows_zip_has_executable_and_licenses_without_personal_data(self):
        with patch.object(dist, "system", return_value="windows"), patch.object(dist, "arch", return_value="x86_64"):
            binary = self.binary("windows")
            secret = self.root / ".amigo/credentials.key"
            secret.parent.mkdir()
            secret.write_bytes(b"PRIVATE-DATA")
            dist.dist_dir().mkdir(parents=True)
            dist.package_windows(self.root / "work")
            with zipfile.ZipFile(dist.artifact("zip")) as archive:
                self.assertEqual(archive.read("Amigo/amigo.exe"), binary.read_bytes())
                for name in ("LICENSE", "FONT-LICENSE", "COUNTRY-DATA-LICENSE", "UNICODE-LICENSE.txt"):
                    self.assertIn(f"Amigo/docs/{name}", archive.namelist())
                self.assertFalse(any("\\" in name or "credentials" in name for name in archive.namelist()))

    def test_macos_bundle_metadata_executable_permissions_and_signing(self):
        with (patch.object(dist, "system", return_value="macos"),
              patch.object(dist, "run") as run):
            self.binary("macos", "aarch64")
            bundle = self.root / "Amigo.app"
            dist.macos_bundle(bundle)
            with (bundle / "Contents/Info.plist").open("rb") as handle:
                info = plistlib.load(handle)
            self.assertEqual(info["CFBundleExecutable"], "amigo")
            self.assertEqual(info["CFBundleShortVersionString"], "1.2.3")
            self.assertEqual(info["LSMinimumSystemVersion"], "12.0.0")
            if os.name != "nt":
                self.assertTrue((bundle / "Contents/MacOS/amigo").stat().st_mode & 0o111)
            self.assertTrue((bundle / "Contents/Resources/docs/UNICODE-LICENSE.txt").exists())
            self.assertIn(("codesign", "--force", "--sign", "-", str(bundle)), [call.args for call in run.call_args_list])
            self.assertEqual(run.call_args.args[:3], ("codesign", "--verify", "--strict"))

    def test_installer_uses_matching_architecture_and_native_output_name(self):
        for architecture, installer_arch in (("x86_64", "x64compatible"), ("aarch64", "arm64")):
            with (self.subTest(architecture=architecture), patch.object(dist, "system", return_value="windows"),
                  patch.object(dist, "arch", return_value=architecture), patch.object(dist, "run") as run,
                  patch.object(dist, "find_tool", return_value="ISCC.exe")):
                self.binary("windows", architecture)
                dist.package_installer(self.root / architecture)
                self.assertIn(f"/DAppArchitecture={installer_arch}", run.call_args.args)
                self.assertIn(f"/DOutputName=amigo-1.2.3-windows-{architecture}.setup", run.call_args.args)

    def test_release_metadata_names_do_not_collide_between_platforms(self):
        names = set()
        for platform in ("linux", "macos", "windows"):
            with (patch.object(dist, "system", return_value=platform),
                  patch.object(dist, "arch", return_value="x86_64"),
                  patch.object(dist, "glibc_version", return_value="2.34") as glibc,
                  patch.object(dist, "source_state", return_value={"commit": "abc", "clean": True})):
                dist.dist_dir().mkdir(parents=True)
                self.binary(platform)
                assets = dist.release_assets()
                for path in assets:
                    path.write_bytes(b"synthetic package")
                dist.prepare()
                state = json.loads(dist.manifest_path().read_text())
                self.assertEqual(state["platform"], platform)
                self.assertEqual(glibc.called, platform == "linux")
                dist.verify_prepared()
                for name in [path.name for path in assets] + [dist.manifest_path().name, dist.checksum_path().name]:
                    self.assertNotIn(name, names)
                    names.add(name)


if __name__ == "__main__":
    unittest.main()
