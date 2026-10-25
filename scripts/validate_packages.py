#!/usr/bin/env python3
"""Validate prepared native packages, or downloaded CI artifact directories.

--native additionally opens AppImages/DMGs or installs and uninstalls Windows
setup in a temporary directory. Use it on disposable native CI runners.
"""

import argparse
import io
import json
from pathlib import Path
import plistlib
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
import xml.etree.ElementTree as ET
import zipfile

import distribute as dist


def require(condition, message):
    if not condition:
        raise ValueError(message)


def architecture(data, platform):
    if platform == "linux" and data[:6] == b"\x7fELF\x02\x01":
        return {62: "x86_64", 183: "aarch64"}.get(struct.unpack_from("<H", data, 18)[0])
    if platform == "macos" and data[:4] == b"\xcf\xfa\xed\xfe":
        return {0x1000007: "x86_64", 0x100000c: "aarch64"}.get(struct.unpack_from("<I", data, 4)[0])
    if platform == "windows" and data[:2] == b"MZ":
        offset = struct.unpack_from("<I", data, 60)[0]
        if data[offset:offset + 4] == b"PE\0\0":
            return {0x8664: "x86_64", 0xaa64: "aarch64"}.get(struct.unpack_from("<H", data, offset + 4)[0])
    return None


def documents(read, prefix):
    for source, name in dist.documentation():
        require(read(prefix + name).decode("utf-8").replace("\r\n", "\n") ==
                source.read_text(encoding="utf-8"), f"Bundled document differs: {name}")


def deb_part(path, prefix):
    parts = dist.output("ar", "t", str(path)).splitlines()
    matches = [name for name in parts if name.startswith(prefix)]
    require(len(matches) == 1, f"Invalid DEB {prefix}: {path}")
    payload = subprocess.check_output(["ar", "p", str(path), matches[0]])
    # Ubuntu's dpkg defaults to zstd; Python's tarfile gains it only in 3.14.
    if matches[0].endswith(".zst"):
        payload = subprocess.run(["zstd", "--decompress", "--stdout"], input=payload,
                                 stdout=subprocess.PIPE, check=True).stdout
    return payload


def rpm_payload(path):
    """Read newc records without extracting paths or depending on cpio."""
    payload = subprocess.check_output(["rpm2cpio", str(path)])
    files, offset = {}, 0
    while offset + 110 <= len(payload):
        header = payload[offset:offset + 110]
        require(header[:6] in (b"070701", b"070702"), "Invalid RPM cpio payload")
        size, namesize = int(header[54:62], 16), int(header[94:102], 16)
        name = payload[offset + 110:offset + 110 + namesize - 1].decode()
        offset = (offset + 110 + namesize + 3) & ~3
        if name == "TRAILER!!!":
            return files
        require(offset + size <= len(payload), "Truncated RPM cpio payload")
        if int(header[14:22], 16) & 0o170000 == 0o100000:
            files[name.removeprefix("./")] = payload[offset:offset + size]
        offset = (offset + size + 3) & ~3
    raise ValueError("Missing RPM cpio trailer")


def elf_sections(binary, relocated=False):
    """Compare loaded ELF sections; RPM may strip nonloaded symbol tables again."""
    require(binary[:6] == b"\x7fELF\x02\x01", "Expected little-endian ELF64")
    offset = struct.unpack_from("<Q", binary, 40)[0]
    size, count = struct.unpack_from("<HH", binary, 58)
    sections = []
    for index in range(count):
        kind, flags, address, start, length = struct.unpack_from("<IQQQQ", binary, offset + index * size + 4)
        if flags & 2:  # SHF_ALLOC: retained program data and code, including the build ID.
            if relocated and kind in (3, 6):  # patchelf adds AppImage RPATH to DYNSTR / DYNAMIC.
                continue
            payload = binary[start:start + length] if kind != 8 else b""
            if relocated and kind == 11:  # DYNSYM section indices follow patchelf's reordered section table.
                payload = bytearray(payload)
                for symbol in range(0, len(payload), 24):
                    payload[symbol + 6:symbol + 8] = b"\0\0"
                payload = bytes(payload)
            sections.append((kind, flags, 0 if relocated else address, length,
                             payload))
    require(sections, "ELF has no allocated sections")
    return sorted(sections)


def linux_packages(folder, stem, arch, version, native):
    with tarfile.open(fileobj=io.BytesIO(deb_part(folder / (stem + ".deb"), "data.tar"))) as archive:
        members = {item.name.removeprefix("./"): item for item in archive.getmembers()}

        def read(name):
            return archive.extractfile(members[name]).read()

        binary = read("usr/bin/amigo")
        require(architecture(binary, "linux") == arch, "DEB executable architecture mismatch")
        require(members["usr/bin/amigo"].mode & 0o111, "DEB executable is not executable")
        documents(read, "usr/share/doc/amigo/")
        metadata = read("usr/share/metainfo/io.github.0x626f.amigo.metainfo.xml")
        component = ET.fromstring(metadata)
        require(component.findtext("name") == "Amigo" and component.findtext("developer/name") == "0x626f"
                and component.findtext("project_license") == "GPL-3.0-or-later"
                and component.find("releases/release").get("version") == version,
                "Incorrect AppStream identity/version")
        rpm = str(folder / (stem + ".rpm"))
        identity = dist.output("rpm", "-qp", "--qf", "%{NAME}|%{VERSION}|%{ARCH}|%{LICENSE}", rpm)
        require(identity == f"amigo|{version}|{arch}|GPL-3.0-or-later", "RPM identity mismatch")
        dist.run("rpm", "--checksig", "--nosignature", rpm)
        rpm_files = rpm_payload(rpm)
        require(architecture(rpm_files["usr/bin/amigo"], "linux") == arch, "RPM executable architecture mismatch")
        for name, member in members.items():
            if member.isfile():
                require(elf_sections(rpm_files[name]) == elf_sections(binary) if name == "usr/bin/amigo"
                        else rpm_files.get(name) == read(name),
                        f"RPM and DEB payload differ: {name}")
    with tarfile.open(fileobj=io.BytesIO(deb_part(folder / (stem + ".deb"), "control.tar"))) as archive:
        member, = [item for item in archive.getmembers() if item.name.removeprefix("./") == "control"]
        fields = dict(line.split(": ", 1) for line in archive.extractfile(member).read().decode().splitlines()
                      if ": " in line and not line.startswith(" "))
        require(fields["Version"] == version and fields["Architecture"] ==
                {"x86_64": "amd64", "aarch64": "arm64"}[arch], "DEB version/architecture mismatch")
        for library in ("libwayland-client0", "libwayland-cursor0", "libwayland-egl1"):
            require(library in fields["Depends"], f"DEB missing dependency: {library}")
    image = folder / (stem + ".AppImage")
    require(architecture(image.read_bytes()[:64], "linux") == arch, "AppImage runtime architecture mismatch")
    if native:
        with tempfile.TemporaryDirectory(prefix="amigo-validate-appimage-") as temp:
            image.chmod(0o755)
            dist.run(str(image.resolve()), "--appimage-extract", cwd=temp, stdout=subprocess.DEVNULL)
            root = Path(temp) / "squashfs-root"
            image_binary = (root / "usr/bin/amigo").read_bytes()
            require(architecture(image_binary, "linux") == arch and
                    elf_sections(image_binary, relocated=True) == elf_sections(binary, relocated=True),
                    "AppImage executable differs from DEB/RPM")
            documents(lambda name: (root / name).read_bytes(), "usr/share/doc/amigo/")
            require(not list((root / "usr/lib").glob("libwayland-*.so*")), "AppImage bundles host Wayland libraries")


def zipped_package(path, platform, arch, version):
    with zipfile.ZipFile(path) as archive:
        require(archive.testzip() is None, f"Corrupt ZIP: {path}")
        require(len(archive.namelist()) == len(set(archive.namelist())), f"Duplicate ZIP entry: {path}")
        require(all(not name.startswith(("/", "\\")) and ".." not in name.replace("\\", "/").split("/")
                    for name in archive.namelist()), f"Unsafe archive path: {path}")
        if platform == "macos":
            info = plistlib.loads(archive.read("Amigo.app/Contents/Info.plist"))
            require(info["CFBundleShortVersionString"] == version and info["CFBundleVersion"] == version
                    and info["CFBundleIdentifier"] == "io.github.0x626f.amigo", "macOS bundle identity mismatch")
            binary = archive.read("Amigo.app/Contents/MacOS/amigo")
            prefix = "Amigo.app/Contents/Resources/docs/"
        else:
            binary = archive.read("Amigo/amigo.exe")
            prefix = "Amigo/docs/"
        require(architecture(binary, platform) == arch, f"ZIP executable architecture mismatch: {path}")
        documents(archive.read, prefix)
        return binary


def macos_native(folder, stem, binary):
    with tempfile.TemporaryDirectory(prefix="amigo-validate-macos-") as temp:
        root = Path(temp)
        dist.run("ditto", "-x", "-k", str(folder / (stem + ".app.zip")), str(root / "zip"))
        dist.run("codesign", "--verify", "--deep", "--strict", str(root / "zip/Amigo.app"))
        mount = root / "volume"
        dist.run("hdiutil", "verify", str(folder / (stem + ".dmg")))
        dist.run("hdiutil", "attach", "-readonly", "-nobrowse", "-mountpoint", str(mount),
                 str(folder / (stem + ".dmg")))
        try:
            dist.run("codesign", "--verify", "--deep", "--strict", str(mount / "Amigo.app"))
            require((mount / "Amigo.app/Contents/MacOS/amigo").read_bytes() == binary,
                    "DMG executable differs from ZIP")
            documents(lambda name: (mount / name).read_bytes(), "Amigo.app/Contents/Resources/docs/")
            require((mount / "Applications").is_symlink(), "DMG lacks Applications link")
        finally:
            dist.run("hdiutil", "detach", str(mount))


def windows_native(folder, stem, binary):
    with tempfile.TemporaryDirectory(prefix="amigo-validate-setup-") as temp:
        app = Path(temp) / "Amigo"
        dist.run(str(folder / (stem + ".setup.exe")), "/VERYSILENT", "/SUPPRESSMSGBOXES",
                 "/NORESTART", "/NOICONS", f"/DIR={app}")
        print("Windows setup completed; checking installed payload.", flush=True)
        try:
            require((app / "amigo.exe").read_bytes() == binary, "Installed executable differs from portable ZIP")
            documents(lambda name: (app / name).read_bytes(), "docs/")
        finally:
            dist.run(str(app / "unins000.exe"), "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART")
            # Inno's launcher can exit before its copied cleanup process removes
            # the original uninstaller. Do not race TemporaryDirectory cleanup.
            wait_removed(app)
        print("Windows uninstall and asynchronous cleanup completed.", flush=True)


def wait_removed(path, timeout=30):
    deadline = time.monotonic() + timeout
    while path.exists() and time.monotonic() < deadline:
        time.sleep(0.2)
    require(not path.exists(), f"Uninstaller left application files: {path}")


def validate(folder, commit, native=False):
    manifests = list(folder.glob("SOURCE-*.json"))
    require(len(manifests) == 1, f"Expected one source manifest in {folder}")
    state = json.loads(manifests[0].read_text())
    version, platform, arch = state["version"], state["platform"], state["architecture"]
    require(version == dist.version() and state["commit"] == commit and state["clean"] is True,
            f"Version/source commit/clean-state mismatch: {manifests[0]}")
    require(platform in dist.PLATFORM_FORMATS and arch in ("x86_64", "aarch64"), "Unknown package target")
    if native:
        require((platform, arch) == (dist.system(), dist.arch()), "Native validation requires matching OS/architecture")
    stem = f"amigo-{version}-{platform}-{arch}"
    packages = {stem + "." + dist.SUFFIXES[kind] for kind in dist.PLATFORM_FORMATS[platform]}
    checksums = {}
    for line in (folder / f"SHA256SUMS-{platform}-{arch}").read_text().splitlines():
        digest, name = line.split("  ", 1)
        require(name not in checksums, f"Duplicate checksum: {name}")
        checksums[name] = digest
    require(checksums.keys() == packages | {manifests[0].name}, "Checksum asset set mismatch")
    for name, digest in checksums.items():
        require((folder / name).stat().st_size > 0 and dist.digest(folder / name) == digest,
                f"Checksum/size mismatch: {name}")
    if platform == "linux":
        linux_packages(folder, stem, arch, version, native)
    else:
        suffix = ".app.zip" if platform == "macos" else ".zip"
        binary = zipped_package(folder / (stem + suffix), platform, arch, version)
        if native:
            {"macos": macos_native, "windows": windows_native}[platform](folder, stem, binary)
    print(f"Validated {platform}/{arch}: {len(packages)} packages, checksums, source {commit[:7]}, contents"
          + (", native container/installer" if native else ""), flush=True)
    return packages | {manifests[0].name, f"SHA256SUMS-{platform}-{arch}"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path, help="Prepared platform directory or downloaded CI artifacts")
    parser.add_argument("--commit", default=None, help="Expected source commit (default: HEAD)")
    parser.add_argument("--native", action="store_true", help="Open/install containers on a disposable native runner")
    args = parser.parse_args()
    commit = args.commit or dist.output("git", "rev-parse", "HEAD")
    folders = sorted({path.parent for path in args.directory.resolve().rglob("SOURCE-*.json")})
    require(folders, "No prepared package manifests found")
    for folder in folders:
        validate(folder, commit, args.native)


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, struct.error, tarfile.TarError, subprocess.CalledProcessError) as error:
        sys.exit(f"Error: {error}")
