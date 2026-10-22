"""Release integrity checks; use temporary synthetic assets and no GitHub calls."""

import contextlib
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import distribute as dist


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="amigo-release-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "1.2.3"\nlicense = "GPL-3.0-or-later"\n')
        self.enterContext(patch.object(dist, "ROOT", self.root))
        self.enterContext(patch.object(dist, "arch", return_value="x86_64"))
        self.enterContext(patch.object(dist, "system", return_value="linux"))
        self.enterContext(patch.object(dist, "existing_release", return_value=None))
        self.enterContext(patch.object(dist, "glibc_version", return_value="2.34"))
        self.enterContext(patch.object(dist, "source_state", return_value={"commit": "abc123", "clean": True}))
        self.enterContext(contextlib.redirect_stdout(io.StringIO()))
        dist.dist_dir().mkdir(parents=True)
        for suffix in (dist.SUFFIXES[kind] for kind in dist.native_formats()):
            dist.artifact(suffix).write_bytes(f"synthetic {suffix}".encode())

    def test_prepare_requires_every_format(self):
        dist.artifact("deb").unlink()
        with self.assertRaisesRegex(ValueError, "Missing package"):
            dist.prepare()
        self.assertFalse(dist.checksum_path().exists())

    def test_checksum_rejects_modified_package_and_provenance(self):
        dist.prepare()
        assets, state = dist.verify_prepared()
        self.assertEqual(len(assets), 4)
        self.assertEqual(state["version"], "1.2.3")
        for path in (dist.artifact("rpm"), dist.manifest_path()):
            original = path.read_bytes()
            path.write_bytes(original + b"tampered")
            with self.assertRaisesRegex(ValueError, "assets changed"):
                dist.verify_prepared()
            path.write_bytes(original)

    def test_prepare_preserves_edited_notes(self):
        dist.prepare()
        notes = dist.dist_dir() / "RELEASE_NOTES.md"
        notes.write_text("My release notes\n")
        dist.prepare()
        self.assertEqual(notes.read_text(), "My release notes\n")

    def test_draft_checks_tag_clean_source_and_notes_before_upload(self):
        dist.prepare()
        with (patch.object(dist, "run") as upload,
              patch.object(dist, "output", return_value="abc123"),
              patch.object(dist, "release_remote", return_value="https://github.com/example/amigo.git")):
            with self.assertRaisesRegex(ValueError, "Tag must match"):
                dist.draft("v9.9.9")
            with patch.object(dist, "source_state", return_value={"commit": "different", "clean": True}):
                with self.assertRaisesRegex(ValueError, "match release-prepare"):
                    dist.draft("v1.2.3")
            with patch.object(dist, "source_state", return_value={"commit": "abc123", "clean": False}):
                with self.assertRaisesRegex(ValueError, "committed, clean"):
                    dist.draft("v1.2.3")
            with patch.object(dist, "output", return_value="different"):
                with self.assertRaisesRegex(ValueError, "tag must point"):
                    dist.draft("v1.2.3")
            with self.assertRaisesRegex(ValueError, "Write release notes"):
                dist.draft("v1.2.3")
            upload.assert_not_called()
            (dist.dist_dir() / "RELEASE_NOTES.md").write_text("First release.\n")
            dist.draft("v1.2.3")
            args = upload.call_args.args
            self.assertEqual(args[:4], ("gh", "release", "create", "v1.2.3"))
            self.assertIn("--draft", args)
            self.assertIn("--verify-tag", args)
            self.assertIn("https://github.com/example/amigo.git", args)
            for suffix in (dist.SUFFIXES[kind] for kind in dist.native_formats()):
                self.assertIn(str(dist.artifact(suffix)), args)

    def test_remote_annotated_tag_must_resolve_to_prepared_commit(self):
        with patch.object(dist, "output", side_effect=["https://github.com/example/amigo.git",
                          "tag-object\trefs/tags/v1.2.3\nabc123\trefs/tags/v1.2.3^{}"]):
            self.assertEqual(dist.release_remote("v1.2.3", "abc123"), "https://github.com/example/amigo.git")
        for refs in ("", "wrong-commit\trefs/tags/v1.2.3"):
            with patch.object(dist, "output", side_effect=["https://github.com/example/amigo.git", refs]):
                with self.assertRaisesRegex(ValueError, "Push the matching release tag"):
                    dist.release_remote("v1.2.3", "abc123")

    def test_uncommitted_preparation_cannot_be_uploaded(self):
        with patch.object(dist, "source_state", return_value={"commit": None, "clean": False}):
            dist.prepare()
        with self.assertRaisesRegex(ValueError, "committed, clean"):
            dist.draft("v1.2.3")

    def test_other_platforms_join_a_draft_without_overwriting_or_publishing(self):
        dist.prepare()
        (dist.dist_dir() / "RELEASE_NOTES.md").write_text("First release.\n")
        with (patch.object(dist, "output", return_value="abc123"),
              patch.object(dist, "release_remote", return_value="https://github.com/example/amigo.git"),
              patch.object(dist, "run") as upload):
            with patch.object(dist, "existing_release", return_value={"isDraft": False}):
                with self.assertRaisesRegex(ValueError, "published release"):
                    dist.draft("v1.2.3")
                upload.assert_not_called()
            with patch.object(dist, "existing_release", return_value={"isDraft": True}):
                dist.draft("v1.2.3")
                self.assertEqual(upload.call_args.args[:4], ("gh", "release", "upload", "v1.2.3"))
                self.assertNotIn("--clobber", upload.call_args.args)

    def test_native_deb_architecture_and_glibc_dependency(self):
        root = self.root / "stage"
        root.mkdir()
        with patch.dict(os.environ, {"AMIGO_MAINTAINER": "Example <example@example.com>"}):
            with patch.object(dist, "run") as builder:
                dist.package_deb(self.root, root)
        control = (root / "DEBIAN/control").read_text()
        self.assertIn("Package: amigo\n", control)
        self.assertIn("Architecture: amd64\n", control)
        self.assertIn("libc6 (>= 2.34)", control)
        self.assertIn("libxkbcommon-x11-0", control)
        self.assertIn("--root-owner-group", builder.call_args.args)

    def test_staged_amigo_contains_launchers_and_icons_without_personal_data(self):
        sources = [
            "target/release/amigo", "packaging/amigo.desktop", "packaging/amigo.svg", "packaging/amigo.png", "README.md",
            "docs/reference-data.md", "crates/core/COUNTRY-DATA-LICENSE", "crates/core/src/countries.tsv", "crates/ui/assets/fonts/LICENSE", "crates/connect/data/UNICODE-LICENSE.txt", "crates/connect/data/README.md", "LICENSE",
        ]
        for name in sources:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("synthetic packaging asset")
        secret = self.root / ".amigo/vaults/private.md"
        secret.parent.mkdir(parents=True)
        secret.write_text("PRIVATE-RECORD")
        with patch.object(dist, "metadata", return_value=("MIT", self.root / "LICENSE")), patch.object(dist, "run"):
            dist.stage(self.root / "AppDir")
        staged = self.root / "AppDir"
        for name in ["usr/bin/amigo", "usr/share/applications/amigo.desktop", "usr/share/icons/hicolor/scalable/apps/amigo.svg", "usr/share/icons/hicolor/256x256/apps/amigo.png"]:
            self.assertTrue((staged / name).is_file())
        self.assertFalse(any(b"PRIVATE-RECORD" in path.read_bytes() for path in staged.rglob("*") if path.is_file()))

    def test_version_validation_prevents_unsafe_output_paths(self):
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "../../outside"\n')
        with self.assertRaisesRegex(ValueError, "stable X.Y.Z"):
            dist.dist_dir()


if __name__ == "__main__":
    unittest.main()
