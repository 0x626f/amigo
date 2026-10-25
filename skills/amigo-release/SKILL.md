---
name: amigo-release
description: "Prepare, validate, and publish Amigo desktop releases using the existing native CI matrix, package checks, and GitHub assets. Use when releasing an Amigo version or validating its distribution packages."
---

# Release Amigo

Resolve the repository root two directories above this skill. Read `scripts/dev.py`,
`scripts/distribute.py`, `.github/workflows/ci.yml`, and the README distribution
section for current commands. Inspect the previous GitHub release with `gh release
view` to match its platforms, assets, notes, and publication state. Treat the
current user request as the authority for preparing, drafting, or publishing;
this skill grants no additional authorization. Do not repeat an approval already
given for the requested release.

## Source and builds

- Inspect the working tree, staged changes, branch, origin, and existing local and
  remote tags. Include completed work in the requested release deliberately;
  preserve unrelated changes. Never reuse old temporary release artifacts.
- Set `[workspace.package].version` in `Cargo.toml`; update the workspace packages
  in `Cargo.lock` without upgrading unrelated dependencies. Update README command
  examples and write versioned notes under `docs/releases/`.
- Run `python3 -B scripts/dev.py check`, `cargo check --locked --examples`, and
  `git diff --check`. Commit the complete release source before packaging so
  `SOURCE-*.json` records a clean, reproducible commit.
- Push the release source to the intended branch. The native workflow runs on
  `main`/`develop` pushes and supports manual dispatch. Follow the run for that
  **exact commit**, not the latest run by timestamp. A run on `develop` is a valid
  release source; do not move `main` unless the requested release workflow calls
  for that branch change.
- The matrix covers Linux/macOS/Windows × x86_64/aarch64. Build natively; reject
  `CARGO_BUILD_TARGET` and mismatched executable headers. Linux CI uses Ubuntu
  22.04 for its glibc baseline. Local Fedora packages may require newer glibc and
  must not replace those CI assets silently.

For a local native build, set the documented Linux `AMIGO_MAINTAINER` when needed
and run `python3 -B scripts/dev.py release-prepare`. CI already performs the
checks, native distribution build, preparation, and container validation.

## Validate the actual packages

Download artifacts only from the successful run for the release commit into a
new temporary directory. Expect six platform/architecture sets: Linux has RPM,
DEB, AppImage; macOS has app ZIP and DMG; Windows has portable ZIP and setup EXE.
That is **14 packages**, six source manifests, and six checksum files: **26
release assets**. `RELEASE_NOTES.md` from each job is not an upload asset.

Run `python3 -B scripts/validate_packages.py ARTIFACT_DIRECTORY --commit SHA`.
It verifies the workspace version, clean source commit, exact checksum sets,
architecture, bundled documents/licenses, and available package contents.
Linux package inspection needs `ar`, `rpm`, and `rpm2cpio`, including for downloaded ARM64
packages. CI also runs it with `--native` on each disposable native runner:
AppImage extraction/payload comparison, macOS DMG mounting and bundle signature
verification, or Windows temporary installation, payload comparison, and uninstall.
Do not run the Windows native install check on a user's everyday installation.

Keep GUI smoke tests separate from structural/container validation. On a Linux
desktop, launch the **downloaded release AppImage** under both Wayland and X11
with temporary `AMIGO_HOME`, `AMIGO_DATA_FILE`, and `XDG_DATA_HOME`; never touch
the user's real library. Check that the process remains alive and creates the
isolated library, then terminate it. Exercise the extracted RPM/DEB executables
where feasible. Both Linux CI jobs install and verify RPMs in clean Fedora
containers. Report native GUI testing only for platforms actually exercised;
macOS signatures or a Windows install check do not establish GUI behavior.

## Tag, upload, and publish

Create/push one annotated `vX.Y.Z` tag pointing to the verified source commit.
If the tag or release already exists, inspect it; do not force-move tags, delete
releases, overwrite assets, or retry publication blindly.

For local single-platform uploads, use `python3 -B scripts/dev.py release-draft
vX.Y.Z`; it enforces clean source, checksums, and matching local/remote tags.
For the complete CI matrix, gather the 26 verified files into a flat temporary
asset directory without altering their bytes. Create one GitHub draft with
`gh release create TAG --draft --verify-tag --title 'Amigo vX.Y.Z'
--notes-file NOTES` plus the explicit asset paths. Never use `--clobber`.

Before publishing, compare the draft's exact asset names, sizes, and GitHub
SHA-256 digests against the verified files; recheck tag/commit and release notes.
Notes should summarize visible changes, show platform downloads, state actual
runtime baselines, signing/notarization status, and distinguish CI/container
validation from desktop tests. Publish when the user's request authorizes it,
then verify public release state and assets again. If an external operation
fails, inspect remote state before retrying; stop if the intended commit or asset
identity cannot be established.

Finish with the release URL, validation outcome and material platform limits,
source commit, and any remaining blocked work.
