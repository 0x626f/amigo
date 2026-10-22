#!/usr/bin/env python3
"""Native Linux, macOS, and Windows packages; deliberate draft-only uploads."""

import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import struct
import sys
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
PLATFORM_FORMATS = {
    "linux": ("rpm", "deb", "appimage"),
    "macos": ("macos", "dmg"),
    "windows": ("windows", "installer"),
}
FORMATS = tuple(kind for formats in PLATFORM_FORMATS.values() for kind in formats)
TOOLS = {"rpm": "rpmbuild", "deb": "dpkg-deb", "appimage": "linuxdeploy",
         "macos": "ditto", "dmg": "hdiutil", "installer": "iscc"}
SUFFIXES = {"rpm": "rpm", "deb": "deb", "appimage": "AppImage", "macos": "app.zip",
            "dmg": "dmg", "windows": "zip", "installer": "setup.exe"}
MACOS_MINIMUM = "12.0"
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
    return subprocess.check_output(args, text=True, encoding="utf-8").strip()


def version():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    value = manifest["workspace"]["package"]["version"]
    # Stable releases only: Debian and RPM use different prerelease version rules.
    if not re.fullmatch(r"\d+\.\d+\.\d+", value):
        raise ValueError("Packaging currently supports stable X.Y.Z workspace versions.")
    return value


def system():
    value = {"Linux": "linux", "Darwin": "macos", "Windows": "windows"}.get(platform.system())
    if value is None:
        raise ValueError("Packaging supports Linux, macOS, and Windows.")
    return value


def arch():
    value = {"amd64": "x86_64", "arm64": "aarch64"}.get(platform.machine().lower(), platform.machine().lower())
    if value not in ("x86_64", "aarch64"):
        raise ValueError("Packaging supports native x86_64 and aarch64 builds.")
    if os.environ.get("CARGO_BUILD_TARGET"):
        raise ValueError("Unset CARGO_BUILD_TARGET: these recipes package native builds.")
    return value


def dist_dir():
    return ROOT / "dist" / version() / system() / arch()


def artifact(suffix):
    return dist_dir() / f"amigo-{version()}-{system()}-{arch()}.{suffix}"


def native_formats():
    return PLATFORM_FORMATS[system()]


def release_assets():
    return [artifact(SUFFIXES[kind]) for kind in native_formats()]


def manifest_path():
    return dist_dir() / f"SOURCE-{system()}-{arch()}.json"


def checksum_path():
    return dist_dir() / f"SHA256SUMS-{system()}-{arch()}"


def binary_path():
    return ROOT / "target/release" / ("amigo.exe" if system() == "windows" else "amigo")


def find_tool(name):
    found = shutil.which(name)
    if found or name != "iscc":
        return found
    # The Inno Setup installer does not always add its compiler to PATH.
    for key in ("ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"):
        base = os.environ.get(key)
        if base:
            for relative in ("Inno Setup 6/ISCC.exe", "Inno Setup 7/ISCC.exe",
                             "Programs/Inno Setup 6/ISCC.exe", "Programs/Inno Setup 7/ISCC.exe"):
                candidate = Path(base) / relative
                if candidate.is_file():
                    return str(candidate)
    return None


def metadata():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    license_name = os.environ.get("AMIGO_LICENSE", manifest["workspace"]["package"].get("license", ""))
    license_file = Path(os.environ.get("AMIGO_LICENSE_FILE", str(ROOT / "LICENSE")))
    if not re.fullmatch(r"[A-Za-z0-9(). +\-]+", license_name):
        raise ValueError("Set AMIGO_LICENSE to your chosen license expression (see README.md).")
    if not license_file.is_file():
        raise ValueError("Add your application LICENSE or set AMIGO_LICENSE_FILE to its path.")
    return license_name, license_file.resolve()


def preflight(formats):
    if not formats or any(kind not in native_formats() for kind in formats):
        raise ValueError(f"{system()} supports these package formats: {', '.join(native_formats())}.")
    arch()
    version()
    metadata()
    if "deb" in formats:
        maintainer = os.environ.get("AMIGO_MAINTAINER", "")
        if not re.fullmatch(r"[^\r\n<>]+ <[^\s<>@]+@[^\s<>@]+>", maintainer):
            raise ValueError("Set AMIGO_MAINTAINER='Your Name <you@example.com>' for DEB packages.")
    tools = {"cargo", "rustc"}
    tools.update(TOOLS[item] for item in formats if item in TOOLS)
    if system() == "linux":
        tools.update(("readelf", "strip", "desktop-file-validate"))
    elif system() == "macos":
        tools.add("codesign")
    if "appimage" in formats:
        tools.add("ldconfig")
    missing = sorted(tool for tool in tools if not find_tool(tool))
    if missing:
        raise ValueError(f"Missing tools: {', '.join(missing)}. See README.md, Distribution recipes.")
    host = next((line[6:] for line in output("rustc", "-vV").splitlines() if line.startswith("host: ")), "")
    targets = {"linux": "unknown-linux-gnu", "macos": "apple-darwin", "windows": "pc-windows-msvc"}
    if host != f"{arch()}-{targets[system()]}":
        raise ValueError(f"Use the native {arch()}-{targets[system()]} Rust toolchain for packaging (found {host}).")
    if system() == "linux":
        run("desktop-file-validate", str(ROOT / "packaging/amigo.desktop"))
    print(f"Ready: {', '.join(formats)} / {arch()} / {version()}")


def documentation():
    return (
        (ROOT / "README.md", "README.md"),
        (ROOT / "docs/reference-data.md", "reference-data.md"),
        (ROOT / "crates/core/COUNTRY-DATA-LICENSE", "COUNTRY-DATA-LICENSE"),
        (ROOT / "crates/core/src/countries.tsv", "countries.tsv"),
        (ROOT / "crates/ui/assets/fonts/LICENSE", "FONT-LICENSE"),
        (ROOT / "crates/connect/data/UNICODE-LICENSE.txt", "UNICODE-LICENSE.txt"),
        (ROOT / "crates/connect/data/README.md", "unicode-data.md"),
        (metadata()[1], "LICENSE"),
    )


def copy_documentation(destination):
    destination.mkdir(parents=True, exist_ok=True)
    for source, name in documentation():
        shutil.copyfile(source, destination / name)


def stage(destination):
    """Copy an explicit allowlist; never include user libraries or vaults."""
    files = (
        (ROOT / "target/release/amigo", "usr/bin/amigo"),
        (ROOT / "packaging/amigo.desktop", "usr/share/applications/amigo.desktop"),
        (ROOT / "packaging/amigo.svg", "usr/share/icons/hicolor/scalable/apps/amigo.svg"),
        (ROOT / "packaging/amigo.png", "usr/share/icons/hicolor/256x256/apps/amigo.png"),
    )
    for source, relative in files:
        dest = destination / relative
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, dest)
        dest.chmod(0o755 if relative == "usr/bin/amigo" else 0o644)
    copy_documentation(destination / "usr/share/doc/amigo")
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
        "Description: Personal tasks, calendars, and finances\n"
        " A native desktop workspace for personal Kanban boards, calendars, and finance vaults.\n"
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
    env = dict(os.environ, ARCH=arch(), LINUXDEPLOY_OUTPUT_VERSION=version(), LDAI_OUTPUT=str(artifact("AppImage")),
               APPIMAGE_EXTRACT_AND_RUN="1")
    run(*args, "--output", "appimage", env=env, cwd=work)
    artifact("AppImage").chmod(0o755)


def validate_binary():
    """Reject stale executables from another target, even with Cargo config overrides."""
    with binary_path().open("rb") as handle:
        header = handle.read(64)
        found = None
        if system() == "linux" and len(header) >= 20 and header[:6] == b"\x7fELF\x02\x01":
            found = {62: "x86_64", 183: "aarch64"}.get(struct.unpack_from("<H", header, 18)[0])
        elif system() == "macos" and len(header) >= 32 and header[:4] == b"\xcf\xfa\xed\xfe":
            found = {0x1000007: "x86_64", 0x100000c: "aarch64"}.get(struct.unpack_from("<I", header, 4)[0])
        elif system() == "windows" and header[:2] == b"MZ" and len(header) == 64:
            handle.seek(struct.unpack_from("<I", header, 60)[0])
            pe = handle.read(6)
            if len(pe) == 6 and pe[:4] == b"PE\0\0":
                found = {0x8664: "x86_64", 0xaa64: "aarch64"}.get(struct.unpack_from("<H", pe, 4)[0])
    if found != arch():
        raise ValueError(f"Expected a native {system()} {arch()} executable at {binary_path()}. Run the build recipe first.")


def macos_minimum():
    """Read the executable's deployment target instead of guessing from the SDK."""
    data = binary_path().read_bytes()
    if len(data) < 32 or data[:4] != b"\xcf\xfa\xed\xfe":
        raise ValueError("Expected a 64-bit macOS executable.")
    count = struct.unpack_from("<I", data, 16)[0]
    offset = 32
    for _ in range(count):
        if offset + 8 > len(data):
            break
        command, size = struct.unpack_from("<II", data, offset)
        if size < 8 or offset + size > len(data):
            break
        version_offset = {0x32: 12, 0x24: 8}.get(command)
        if version_offset is not None and size >= version_offset + 4:
            if command == 0x32 and struct.unpack_from("<I", data, offset + 8)[0] != 1:
                raise ValueError("The executable targets an Apple platform other than macOS.")
            value = struct.unpack_from("<I", data, offset + version_offset)[0]
            return f"{value >> 16}.{(value >> 8) & 0xff}.{value & 0xff}"
        offset += size
    raise ValueError("Cannot determine the executable's minimum macOS version.")


def macos_bundle(destination):
    contents = destination / "Contents"
    executable = contents / "MacOS/amigo"
    executable.parent.mkdir(parents=True)
    shutil.copyfile(binary_path(), executable)
    executable.chmod(0o755)
    resources = contents / "Resources"
    copy_documentation(resources / "docs")
    shutil.copyfile(ROOT / "packaging/amigo.icns", resources / "amigo.icns")
    minimum = macos_minimum()
    with (contents / "Info.plist").open("wb") as handle:
        plistlib.dump({
            "CFBundleName": "Amigo", "CFBundleDisplayName": "Amigo",
            "CFBundleIdentifier": "io.github.0x626f.amigo", "CFBundleExecutable": "amigo",
            "CFBundlePackageType": "APPL", "CFBundleIconFile": "amigo.icns",
            "CFBundleShortVersionString": version(), "CFBundleVersion": version(),
            "LSMinimumSystemVersion": minimum, "NSHighResolutionCapable": True,
            "NSSupportsAutomaticGraphicsSwitching": True,
        }, handle)
    identity = os.environ.get("AMIGO_MACOS_SIGN_IDENTITY", "-")
    args = ["codesign", "--force", "--sign", identity]
    if identity != "-":
        args.extend(("--options", "runtime", "--timestamp"))
    run(*args, str(destination))
    run("codesign", "--verify", "--strict", str(destination))


def package_macos(work):
    app = work / "Amigo.app"
    macos_bundle(app)
    run("ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(artifact("app.zip")))


def package_dmg(work):
    volume = work / "volume"
    macos_bundle(volume / "Amigo.app")
    (volume / "Applications").symlink_to("/Applications", target_is_directory=True)
    run("hdiutil", "create", "-volname", "Amigo", "-srcfolder", str(volume), "-ov",
        "-format", "UDZO", str(artifact("dmg")))


def windows_directory(destination):
    destination.mkdir(parents=True)
    shutil.copyfile(binary_path(), destination / "amigo.exe")
    shutil.copyfile(ROOT / "packaging/amigo.ico", destination / "amigo.ico")
    copy_documentation(destination / "docs")


def package_windows(work):
    app = work / "Amigo"
    windows_directory(app)
    with zipfile.ZipFile(artifact("zip"), "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(app.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(work).as_posix())


def package_installer(work):
    app = work / "Amigo"
    windows_directory(app)
    run(find_tool("iscc"), f"/DAppVersion={version()}", f"/DStageDir={app}",
        f"/DOutputDir={dist_dir()}", f"/DOutputName={artifact('setup.exe').stem}",
        f"/DAppArchitecture={'arm64' if arch() == 'aarch64' else 'x64compatible'}",
        str(ROOT / "packaging/amigo.iss"))


def package(kind):
    preflight((kind,))
    validate_binary()
    dist_dir().mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="amigo-package-") as temp:
        work = Path(temp)
        if system() == "linux":
            root = work / "AppDir"
            stage(root)
            {"rpm": package_rpm, "deb": package_deb, "appimage": package_appimage}[kind](work, root)
        else:
            {"macos": package_macos, "dmg": package_dmg, "windows": package_windows,
             "installer": package_installer}[kind](work)
    print(artifact(SUFFIXES[kind]))


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
    assets = release_assets()
    for path in assets:
        if not path.is_file() or not path.stat().st_size:
            raise ValueError(f"Missing package: {path}. Run just dist first.")
    state = dict(source_state(), version=version(), platform=system(), architecture=arch())
    if system() == "linux":
        state["glibc"] = glibc_version(binary_path())
        requirements = f"Linux {arch()}; requires glibc {state['glibc']} or newer and OpenGL/EGL drivers."
    elif system() == "macos":
        state["minimum_os"] = macos_minimum()
        requirements = f"macOS {state['minimum_os']} or newer; {arch()}."
    else:
        state["minimum_os"] = "Windows 10"
        requirements = f"Windows 10 or newer; {arch()}; OpenGL-capable graphics drivers."
    manifest_path().write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    assets.append(manifest_path())
    checksum_path().write_text("".join(f"{digest(p)}  {p.name}\n" for p in assets), encoding="utf-8")
    notes = dist_dir() / "RELEASE_NOTES.md"
    if not notes.exists():
        notes.write_text(f"# Amigo {version()}\n\n"
                         "Describe the changes in this release here.\n\n"
                         f"{requirements}\n\n"
                         "Download a package matching your operating system and architecture. "
                         f"Verify downloads against `{checksum_path().name}`.\n", encoding="utf-8")
    print(f"Prepared {dist_dir()}\nEdit RELEASE_NOTES.md, then use just release-draft v{version()}.")
    if not state["clean"]:
        print("Source is uncommitted or dirty. Commit it and rerun release-prepare before uploading.")


def verify_prepared():
    assets = release_assets() + [manifest_path()]
    expected = "".join(f"{digest(p)}  {p.name}\n" for p in assets)
    if checksum_path().read_text(encoding="utf-8") != expected:
        raise ValueError("Prepared assets changed. Rerun just release-prepare.")
    state = json.loads(manifest_path().read_text(encoding="utf-8"))
    if state.get("version") != version() or state.get("architecture") != arch() or state.get("platform") != system():
        raise ValueError("Prepared version, platform, or architecture does not match this checkout.")
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


def existing_release(tag, remote):
    result = subprocess.run(("gh", "release", "view", tag, "--repo", remote, "--json", "isDraft"),
                            capture_output=True, text=True, encoding="utf-8")
    if result.returncode:
        # Creating below is safe even if the lookup failed for a transient reason:
        # GitHub rejects duplicate releases, and nothing is overwritten.
        return None
    return json.loads(result.stdout)


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
    if "Describe the changes in this release here." in notes.read_text(encoding="utf-8"):
        raise ValueError(f"Write release notes in {notes} before uploading.")
    remote = release_remote(tag, current["commit"])
    existing = existing_release(tag, remote)
    if existing is not None:
        if not existing["isDraft"]:
            raise ValueError("Refusing to upload to a published release. Use an unpublished draft.")
        # OS/architecture-qualified names let another native build join the draft.
        # No --clobber: existing assets are never overwritten.
        run("gh", "release", "upload", tag, *(str(p) for p in assets), str(checksum_path()), "--repo", remote)
    else:
        run("gh", "release", "create", tag, *(str(p) for p in assets), str(checksum_path()),
            "--repo", remote, "--draft", "--verify-tag", "--title", f"Amigo {version()}",
            "--notes-file", str(notes))


def main():
    os.chdir(ROOT)
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "preflight":
        kind = sys.argv[2] if len(sys.argv) > 2 else "all"
        if kind not in (*FORMATS, "all"):
            raise ValueError(f"Format must be all or one of: {', '.join(FORMATS)}.")
        preflight(native_formats() if kind == "all" else (kind,))
    elif command in FORMATS:
        package(command)
    elif command == "prepare":
        prepare()
    elif command == "draft" and len(sys.argv) == 3:
        draft(sys.argv[2])
    else:
        raise ValueError(f"Usage: distribute.py preflight [format] | {' | '.join(FORMATS)} | prepare | draft TAG")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"Error: {error}")
