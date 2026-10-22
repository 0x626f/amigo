#!/usr/bin/env python3
"""Native Linux packaging and deliberate, draft-only GitHub uploads (Python 3.11+)."""

import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
FORMATS = ("rpm", "deb", "appimage")
TOOLS = {"rpm": "rpmbuild", "deb": "dpkg-deb", "appimage": "linuxdeploy"}
SUFFIXES = ("rpm", "deb", "AppImage")
# winit loads these at runtime, so scanning ELF DT_NEEDED alone misses them.
GUI_LIBRARIES = (
    "libX11.so.6", "libXcursor.so.1", "libXi.so.6", "libXrandr.so.2",
    "libxkbcommon.so.0", "libxkbcommon-x11.so.0", "libwayland-client.so.0",
    "libwayland-cursor.so.0", "libwayland-egl.so.1",
    "libdbus-1.so.3",
)


def run(*args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def output(*args):
    return subprocess.check_output(args, text=True).strip()


def version():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    value = manifest["workspace"]["package"]["version"]
    # Stable releases only: Debian and RPM use different prerelease version rules.
    if not re.fullmatch(r"\d+\.\d+\.\d+", value):
        raise ValueError("Packaging currently supports stable X.Y.Z workspace versions.")
    return value


def arch():
    value = platform.machine()
    if platform.system() != "Linux" or value not in ("x86_64", "aarch64"):
        raise ValueError("Packaging supports native x86_64 and aarch64 Linux builds.")
    if os.environ.get("CARGO_BUILD_TARGET"):
        raise ValueError("Unset CARGO_BUILD_TARGET: these recipes package native builds.")
    return value


def dist_dir():
    return ROOT / "dist" / version() / arch()


def artifact(suffix):
    return dist_dir() / f"amigo-{version()}-{arch()}.{suffix}"


def metadata():
    license_name = os.environ.get("AMIGO_LICENSE", "")
    license_file = Path(os.environ.get("AMIGO_LICENSE_FILE", "LICENSE"))
    if not re.fullmatch(r"[A-Za-z0-9(). +\-]+", license_name):
        raise ValueError("Set AMIGO_LICENSE to your chosen license expression (see README.md).")
    if not license_file.is_file():
        raise ValueError("Add your application LICENSE or set AMIGO_LICENSE_FILE to its path.")
    return license_name, license_file.resolve()


def preflight(formats):
    arch()
    version()
    metadata()
    if "deb" in formats:
        maintainer = os.environ.get("AMIGO_MAINTAINER", "")
        if not re.fullmatch(r"[^\r\n<>]+ <[^\s<>@]+@[^\s<>@]+>", maintainer):
            raise ValueError("Set AMIGO_MAINTAINER='Your Name <you@example.com>' for DEB packages.")
    tools = {"cargo", "readelf", "strip", "desktop-file-validate"}
    tools.update(TOOLS[item] for item in formats)
    if "appimage" in formats:
        tools.add("ldconfig")
    missing = sorted(tool for tool in tools if not shutil.which(tool))
    if missing:
        raise ValueError(f"Missing tools: {', '.join(missing)}. See README.md, Distribution recipes.")
    run("desktop-file-validate", str(ROOT / "packaging/amigo.desktop"))
    print(f"Ready: {', '.join(formats)} / {arch()} / {version()}")


def stage(destination):
    """Copy an explicit allowlist; never include user libraries or vaults."""
    _, license_file = metadata()
    files = (
        (ROOT / "target/release/amigo", "usr/bin/amigo"),
        (ROOT / "packaging/amigo.desktop", "usr/share/applications/amigo.desktop"),
        (ROOT / "packaging/amigo.svg", "usr/share/icons/hicolor/scalable/apps/amigo.svg"),
        (ROOT / "packaging/amigo.png", "usr/share/icons/hicolor/256x256/apps/amigo.png"),
        (ROOT / "README.md", "usr/share/doc/amigo/README.md"),
        (ROOT / "docs/reference-data.md", "usr/share/doc/amigo/reference-data.md"),
        (ROOT / "crates/core/COUNTRY-DATA-LICENSE", "usr/share/doc/amigo/COUNTRY-DATA-LICENSE"),
        (ROOT / "crates/core/src/countries.tsv", "usr/share/doc/amigo/countries.tsv"),
        (ROOT / "crates/ui/assets/fonts/LICENSE", "usr/share/doc/amigo/FONT-LICENSE"),
        (license_file, "usr/share/doc/amigo/LICENSE"),
    )
    for source, relative in files:
        dest = destination / relative
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest)
        dest.chmod(0o755 if relative == "usr/bin/amigo" else 0o644)
    for directory in (destination, *(p for p in destination.rglob("*") if p.is_dir())):
        directory.chmod(0o755)
    run("strip", "--strip-unneeded", str(destination / "usr/bin/amigo"))


def glibc_version(binary):
    versions = re.findall(r"\bGLIBC_(\d+(?:\.\d+)+)\b", output("readelf", "--version-info", str(binary)))
    if not versions:
        raise ValueError("Cannot determine the binary's glibc requirement; GNU/Linux builds only.")
    return max(versions, key=lambda value: tuple(map(int, value.split("."))))


def package_deb(work, root):
    control = root / "DEBIAN"
    control.mkdir()
    minimum = glibc_version(root / "usr/bin/amigo")
    (control / "control").write_text(
        f"Package: amigo\nVersion: {version()}\nSection: utils\nPriority: optional\n"
        f"Architecture: {'amd64' if arch() == 'x86_64' else 'arm64'}\n"
        f"Maintainer: {os.environ['AMIGO_MAINTAINER']}\n"
        f"Installed-Size: {sum(p.stat().st_size for p in root.rglob('*') if p.is_file()) // 1024 + 1}\n"
        f"Depends: libc6 (>= {minimum}), libgcc-s1, libx11-6, libxcursor1, libxi6, libxrandr2, "
        "libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-cursor0, "
        "libwayland-egl1, libgl1, libegl1, libdbus-1-3, xdg-utils, xdg-desktop-portal\n"
        "Description: Personal task boards and finance vaults\n"
        " A native desktop workspace for personal Kanban boards and finance vaults.\n"
    )
    run("dpkg-deb", "--root-owner-group", "--build", str(root), str(artifact("deb")))


def package_rpm(work, root):
    top = work / "rpmbuild"
    for name in ("BUILD", "BUILDROOT", "RPMS", "SOURCES", "SPECS", "SRPMS"):
        (top / name).mkdir(parents=True)
    shutil.copytree(root, top / "SOURCES/root")
    text = (ROOT / "packaging/amigo.spec.in").read_text()
    text = text.replace("@VERSION@", version()).replace("@LICENSE@", metadata()[0])
    spec = top / "SPECS/amigo.spec"
    spec.write_text(text)
    run("rpmbuild", "-bb", "--define", f"_topdir {top}", "--target", arch(), str(spec))
    packages = list((top / "RPMS").rglob("amigo-*.rpm"))
    if len(packages) != 1:
        raise ValueError(f"Expected one RPM, found {len(packages)}.")
    shutil.copyfile(packages[0], artifact("rpm"))


def gui_libraries(binary):
    cache = output("ldconfig", "-p")
    with binary.open("rb") as handle:
        machine = handle.read(20)[18:20]  # ELF e_machine; reject other multilib architectures.
    paths = []
    for name in GUI_LIBRARIES:
        candidates = re.findall(r"^\s*" + re.escape(name) + r"\s+.* => (.+)$", cache, re.M)
        for candidate in candidates:
            path = Path(candidate)
            with path.open("rb") as handle:
                header = handle.read(20)
            if header[:4] == b"\x7fELF" and header[18:20] == machine:
                paths.append(candidate)
                break
        else:
            raise ValueError(f"Install the native runtime library {name} before building an AppImage.")
    return paths


def package_appimage(work, root):
    args = ["linuxdeploy", "--appdir", str(root), "--executable", str(root / "usr/bin/amigo"),
            "--desktop-file", str(ROOT / "packaging/amigo.desktop"),
            "--icon-file", str(ROOT / "packaging/amigo.svg")]
    for library in gui_libraries(root / "usr/bin/amigo"):
        args.extend(("--library", library))
    env = dict(os.environ, ARCH=arch(), LDAI_VERSION=version(), LDAI_OUTPUT=str(artifact("AppImage")),
               APPIMAGE_EXTRACT_AND_RUN="1")
    run(*args, "--output", "appimage", env=env, cwd=work)
    artifact("AppImage").chmod(0o755)


def package(kind):
    preflight((kind,))
    dist_dir().mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="amigo-package-") as temp:
        work = Path(temp)
        root = work / "AppDir"
        stage(root)
        {"rpm": package_rpm, "deb": package_deb, "appimage": package_appimage}[kind](work, root)
    print(artifact({"appimage": "AppImage"}.get(kind, kind)))


def digest(path):
    with path.open("rb") as handle:
        return hashlib.file_digest(handle, "sha256").hexdigest()


def source_state():
    try:
        commit = output("git", "rev-parse", "HEAD")
        clean = not output("git", "status", "--porcelain", "--untracked-files=normal")
        return {"commit": commit, "clean": clean}
    except subprocess.CalledProcessError:
        return {"commit": None, "clean": False}


def prepare():
    assets = [artifact(suffix) for suffix in SUFFIXES]
    for path in assets:
        if not path.is_file() or not path.stat().st_size:
            raise ValueError(f"Missing package: {path}. Run just dist first.")
    state = dict(source_state(), version=version(), architecture=arch())
    state["glibc"] = glibc_version(ROOT / "target/release/amigo")
    (dist_dir() / "SOURCE.json").write_text(json.dumps(state, indent=2) + "\n")
    assets.append(dist_dir() / "SOURCE.json")
    (dist_dir() / "SHA256SUMS").write_text("".join(f"{digest(p)}  {p.name}\n" for p in assets))
    notes = dist_dir() / "RELEASE_NOTES.md"
    if not notes.exists():
        notes.write_text(f"# Amigo {version()}\n\n"
                         "Describe the changes in this release here.\n\n"
                         f"Linux {arch()}; requires glibc {state['glibc']} or newer, "
                         "a desktop session, and working OpenGL/EGL drivers.\n\n"
                         "Download the RPM, DEB, or AppImage for your system. "
                         "Verify downloads using `sha256sum -c SHA256SUMS`.\n")
    print(f"Prepared {dist_dir()}\nEdit RELEASE_NOTES.md, then use just release-draft v{version()}.")
    if not state["clean"]:
        print("Source is uncommitted or dirty. Commit it and rerun release-prepare before uploading.")


def verify_prepared():
    assets = [artifact(suffix) for suffix in SUFFIXES] + [dist_dir() / "SOURCE.json"]
    expected = "".join(f"{digest(p)}  {p.name}\n" for p in assets)
    if (dist_dir() / "SHA256SUMS").read_text() != expected:
        raise ValueError("Prepared assets changed. Rerun just release-prepare.")
    state = json.loads((dist_dir() / "SOURCE.json").read_text())
    if state.get("version") != version() or state.get("architecture") != arch():
        raise ValueError("Prepared version or architecture does not match this checkout.")
    return assets, state


def release_remote(tag, commit):
    remote = output("git", "remote", "get-url", "origin")
    refs = dict(line.split()[::-1] for line in output(
        "git", "ls-remote", "--tags", "origin", f"refs/tags/{tag}", f"refs/tags/{tag}^{{}}"
    ).splitlines())
    remote_commit = refs.get(f"refs/tags/{tag}^{{}}", refs.get(f"refs/tags/{tag}"))
    if remote_commit != commit:
        raise ValueError("Push the matching release tag to origin before uploading.")
    return remote


def draft(tag):
    if tag != f"v{version()}":
        raise ValueError(f"Tag must match the workspace version: v{version()}")
    assets, state = verify_prepared()
    current = source_state()
    if not current["clean"] or not state.get("clean") or current["commit"] != state.get("commit"):
        raise ValueError("Release source must be committed, clean, and match release-prepare. Rebuild after committing.")
    if output("git", "rev-parse", f"refs/tags/{tag}^{{commit}}") != current["commit"]:
        raise ValueError("The release tag must point to the prepared source commit.")
    notes = dist_dir() / "RELEASE_NOTES.md"
    if "Describe the changes in this release here." in notes.read_text():
        raise ValueError(f"Write release notes in {notes} before uploading.")
    remote = release_remote(tag, current["commit"])
    # gh --verify-tag checks the tag exists remotely; never create or push tags implicitly.
    run("gh", "release", "create", tag, *(str(p) for p in assets), str(dist_dir() / "SHA256SUMS"),
        "--repo", remote, "--draft", "--verify-tag", "--title", f"Amigo {version()}",
        "--notes-file", str(notes))


def main():
    os.chdir(ROOT)
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "preflight":
        kind = sys.argv[2] if len(sys.argv) > 2 else "all"
        if kind not in (*FORMATS, "all"):
            raise ValueError("Format must be all, rpm, deb, or appimage.")
        preflight(FORMATS if kind == "all" else (kind,))
    elif command in FORMATS:
        package(command)
    elif command == "prepare":
        prepare()
    elif command == "draft" and len(sys.argv) == 3:
        draft(sys.argv[2])
    else:
        raise ValueError("Usage: distribute.py preflight [format] | rpm | deb | appimage | prepare | draft TAG")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"Error: {error}")
