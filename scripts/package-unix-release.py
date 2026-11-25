#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""Assemble the Linux and macOS release files, with licenses, from finished builds.

It builds nothing. Run it after `cargo build --release --locked -p argentum-app`
and `cargo build --release --locked -p argentum-cli --bin argentum` for the same
target, then point --target-dir at that build folder.

Linux files: Argentum-<v>-linux-x64.tar.gz (desktop app), argentum-genesis-<v>-
linux-x64.tar.gz (CLI) and argentum_<v>_amd64.deb (both). macOS files:
Argentum-<v>-macos-<arch>.tar.gz (Argentum.app, unsigned) and
argentum-genesis-<v>-macos-<arch>.tar.gz (CLI). No file is code signed.
"""

import argparse
import glob
import json
import os
import pathlib
import shutil
import subprocess
import tarfile
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent

PLATFORMS = {
    "linux-x64": {"target": "x86_64-unknown-linux-gnu", "deb_arch": "amd64"},
    "macos-arm64": {"target": "aarch64-apple-darwin"},
    "macos-x64": {"target": "x86_64-apple-darwin"},
}
CLI_DOCS = ("CLI_GUIDE.md", "CHATGPT_SIGN_IN.md", "SUPPORT_MATRIX.md", "NATIVE_DELIVERY.md")
NOTICE_FILES = ("LICENSE", "NOTICE.md", "THIRD_PARTY_NOTICES.md")


def run(*command):
    return subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                          encoding="utf-8", check=True).stdout


def cargo_metadata(target):
    return json.loads(run("cargo", "metadata", "--locked", "--format-version", "1",
                          "--filter-platform", target))


def dependencies(metadata, root_package):
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    start = next(p["id"] for p in metadata["packages"] if p["name"] == root_package)
    seen, queue = set(), [start]
    while queue:
        current = queue.pop()
        if current in seen:
            continue
        seen.add(current)
        for dependency in nodes[current]["deps"]:
            if any(kind["kind"] is None for kind in dependency["dep_kinds"]):
                queue.append(dependency["pkg"])
    return [packages[item] for item in sorted(seen, key=lambda i: packages[i]["name"])
            if packages[item]["source"]]


def copy_notices(stage, metadata, root_package):
    records = []
    for package in dependencies(metadata, root_package):
        source = os.path.dirname(package["manifest_path"])
        label = f"{package['name']}-{package['version']}"
        found = []
        for pattern in ("LICEN[CS]E*", "COPYING*", "NOTICE*", "LICENSES/*", "license*"):
            for path in glob.glob(os.path.join(glob.escape(source), pattern)):
                if os.path.isfile(path) and os.path.basename(path) not in found:
                    target = stage / "licenses" / label
                    target.mkdir(parents=True, exist_ok=True)
                    shutil.copy(path, target)
                    found.append(os.path.basename(path))
        records.append({"name": package["name"], "version": package["version"],
                        "license": package.get("license"),
                        "repository": package.get("repository"), "license_files": found})
    (stage / "dependency-notices.json").write_text(json.dumps(records, indent=2) + "\n",
                                                   encoding="utf-8")
    for item in NOTICE_FILES:
        shutil.copy(ROOT / item, stage)
    return records


def write_provenance(stage, product, version, commit, target):
    (stage / "provenance.json").write_text(
        json.dumps({"product": product, "version": version, "commit": commit,
                    "target": target, "code_signed": False}, indent=2) + "\n",
        encoding="utf-8")


def install_binary(source, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy(source, destination)
    destination.chmod(0o755)


def make_tarball(stage, output, mtime):
    """A gzip tarball with the folder as its top entry, fixed owners and times."""
    def clean(info):
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        info.mtime = mtime
        info.mode = 0o755 if info.isdir() or info.mode & 0o111 else 0o644
        return info
    with tarfile.open(output, "w:gz", format=tarfile.PAX_FORMAT) as archive:
        archive.add(stage, arcname=stage.name, filter=clean)


def desktop_entry():
    return ("[Desktop Entry]\nType=Application\nName=Argentum\n"
            "Comment=Native workbench for an AI coding agent\n"
            "Exec=argentum-desktop\nIcon=argentum\nTerminal=false\n"
            "Categories=Development;IDE;\n")


def info_plist(version):
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Argentum</string>
  <key>CFBundleDisplayName</key><string>Argentum</string>
  <key>CFBundleIdentifier</key><string>eu.ag064.argentum</string>
  <key>CFBundleVersion</key><string>{version}</string>
  <key>CFBundleShortVersionString</key><string>{version}</string>
  <key>CFBundleExecutable</key><string>Argentum</string>
  <key>CFBundleIconFile</key><string>argentum.icns</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
"""


def build_deb(output, version, arch, desktop_binary, cli_binary, mtime):
    with tempfile.TemporaryDirectory() as temporary:
        root = pathlib.Path(temporary) / "root"
        install_binary(cli_binary, root / "usr/bin/argentum")
        install_binary(desktop_binary, root / "usr/bin/argentum-desktop")
        applications = root / "usr/share/applications"
        applications.mkdir(parents=True)
        (applications / "argentum.desktop").write_text(desktop_entry(), encoding="utf-8")
        icons = root / "usr/share/icons/hicolor/256x256/apps"
        icons.mkdir(parents=True)
        shutil.copy(ROOT / "assets/brand/argentum-256.png", icons / "argentum.png")
        doc = root / "usr/share/doc/argentum"
        doc.mkdir(parents=True)
        shutil.copy(ROOT / "LICENSE", doc / "copyright")
        for item in ("NOTICE.md", "THIRD_PARTY_NOTICES.md"):
            shutil.copy(ROOT / item, doc)
        size_kib = sum(p.stat().st_size for p in root.rglob("*") if p.is_file()) // 1024
        control = root / "DEBIAN"
        control.mkdir()
        (control / "control").write_text(
            f"Package: argentum\nVersion: {version}\nArchitecture: {arch}\n"
            "Maintainer: AG064 <contact@ag064.eu>\nSection: devel\nPriority: optional\n"
            f"Installed-Size: {size_kib}\nHomepage: https://github.com/AG064/argentum\n"
            "Depends: libc6 (>= 2.35), libfontconfig1, libxkbcommon0\n"
            "Recommends: libxkbcommon-x11-0, libwayland-client0, git\n"
            "Description: Native workbench and CLI for an AI coding agent\n"
            " Argentum runs a coding agent on your own machine under a persisted\n"
            " workspace policy. This package has the desktop app (argentum-desktop)\n"
            " and the command-line agent (argentum).\n",
            encoding="utf-8")
        for path in root.rglob("*"):
            os.utime(path, (mtime, mtime))
        subprocess.run(["dpkg-deb", "--root-owner-group", "-Zxz", "--build", str(root),
                        str(output)], check=True, env={**os.environ, "SOURCE_DATE_EPOCH": str(mtime)})


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--platform", choices=sorted(PLATFORMS), required=True)
    parser.add_argument("--target-dir", type=pathlib.Path, required=True,
                        help="Cargo build folder; binaries are read from <dir>/<target>/release")
    parser.add_argument("--output", type=pathlib.Path, required=True)
    arguments = parser.parse_args()
    platform = PLATFORMS[arguments.platform]
    target = platform["target"]
    release = arguments.target_dir / target / "release"
    desktop_binary = release / "argentum-app-desktop"
    cli_binary = release / "argentum-cli-bin"
    # Both packages name their binary "argentum"; the build step copies them
    # apart so neither overwrites the other.
    for path in (desktop_binary, cli_binary):
        if not path.is_file():
            raise SystemExit(f"Missing build output {path}")
    metadata = cargo_metadata(target)
    version = next(p["version"] for p in metadata["packages"] if p["name"] == "argentum-app")
    commit = run("git", "rev-parse", "HEAD").strip()
    mtime = int(run("git", "log", "-1", "--format=%ct").strip())
    arguments.output.mkdir(parents=True, exist_ok=True)
    suffix = arguments.platform
    with tempfile.TemporaryDirectory() as temporary:
        temporary = pathlib.Path(temporary)

        desktop = temporary / f"Argentum-{version}-{suffix}"
        desktop.mkdir()
        if suffix.startswith("macos"):
            contents = desktop / "Argentum.app" / "Contents"
            install_binary(desktop_binary, contents / "MacOS" / "Argentum")
            (contents / "Resources").mkdir(parents=True)
            shutil.copy(ROOT / "assets/brand/argentum.icns", contents / "Resources")
            (contents / "Info.plist").write_text(info_plist(version), encoding="utf-8")
        else:
            install_binary(desktop_binary, desktop / "argentum-desktop")
            shutil.copy(ROOT / "assets/brand/argentum-256.png", desktop / "argentum.png")
            (desktop / "argentum.desktop").write_text(desktop_entry(), encoding="utf-8")
        shutil.copy(ROOT / "crates/argentum-app/README.md", desktop / "README.md")
        records = copy_notices(desktop, metadata, "argentum-app")
        write_provenance(desktop, "Argentum desktop", version, commit, target)
        make_tarball(desktop, arguments.output / f"{desktop.name}.tar.gz", mtime)

        cli = temporary / f"argentum-genesis-{version}-{suffix}"
        cli.mkdir()
        install_binary(cli_binary, cli / "argentum")
        shutil.copy(ROOT / "README.md", cli / "README.md")
        (cli / "docs").mkdir()
        for item in CLI_DOCS:
            shutil.copy(ROOT / "docs" / item, cli / "docs" / item)
        copy_notices(cli, metadata, "argentum-cli")
        write_provenance(cli, "Argentum Genesis", version, commit, target)
        make_tarball(cli, arguments.output / f"{cli.name}.tar.gz", mtime)

    if "deb_arch" in platform:
        arch = platform["deb_arch"]
        build_deb(arguments.output / f"argentum_{version}_{arch}.deb", version, arch,
                  desktop_binary, cli_binary, mtime)
    for path in sorted(arguments.output.iterdir()):
        print(f"{path.stat().st_size:>12}  {path.name}")
    missing = [record["name"] for record in records if not record["license_files"]]
    print(f"{len(records)} desktop dependencies, {len(missing)} ship no license file: "
          f"{', '.join(missing)}")


if __name__ == "__main__":
    main()
