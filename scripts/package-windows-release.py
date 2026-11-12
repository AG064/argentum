#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""Assemble the Windows release files, with licenses, from finished builds."""

import argparse
import glob
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent


def cargo_metadata():
    return json.loads(
        subprocess.run(
            ["cargo", "metadata", "--locked", "--format-version", "1",
             "--filter-platform", "x86_64-pc-windows-msvc"],
            cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=True,
        ).stdout
    )


def workspace_version():
    """The release version comes from the workspace Cargo.toml, never a constant."""
    return next(p["version"] for p in cargo_metadata()["packages"] if p["name"] == "argentum-app")


def dependencies():
    metadata = cargo_metadata()
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    start = next(p["id"] for p in metadata["packages"] if p["name"] == "argentum-app")
    seen, queue = set(), [start]
    while queue:
        current = queue.pop()
        if current in seen:
            continue
        seen.add(current)
        for dependency in nodes[current]["deps"]:
            if any(kind["kind"] is None for kind in dependency["dep_kinds"]):
                queue.append(dependency["pkg"])
    return [packages[item] for item in sorted(seen, key=lambda i: packages[i]["name"]) if packages[item]["source"]]


def copy_notices(stage):
    records = []
    for package in dependencies():
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
        records.append({
            "name": package["name"],
            "version": package["version"],
            "license": package.get("license"),
            "repository": package.get("repository"),
            "license_files": found,
        })
    (stage / "dependency-notices.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    return records


def check_runtime_free(folder):
    """A clean PC has no Visual C++ runtime, so a binary that needs it will not start."""
    for path in sorted(folder.iterdir()):
        binaries = []
        if path.suffix == ".exe":
            binaries.append((path.name, path.read_bytes()))
        elif path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                binaries.extend(
                    (f"{path.name}:{name}", archive.read(name))
                    for name in archive.namelist()
                    if name.lower().endswith((".exe", ".dll"))
                )
        for name, data in binaries:
            if b"vcruntime140" in data.lower():
                raise SystemExit(f"{name} needs the Visual C++ runtime")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=pathlib.Path, required=True)
    parser.add_argument("--cli", type=pathlib.Path, required=True)
    parser.add_argument("--cli-desktop", type=pathlib.Path, required=True)
    arguments = parser.parse_args()
    arguments.output.mkdir(parents=True)
    version = workspace_version()
    built = ROOT / "artifacts" / "windows-x64"
    name = f"Argentum-{version}-windows-x64"
    with tempfile.TemporaryDirectory() as temporary:
        stage = pathlib.Path(temporary) / name
        stage.mkdir()
        records = copy_notices(stage)
        for item in ("Argentum.exe", "Argentum.ico", "Argentum.brand.json"):
            shutil.copy(built / item, stage)
        for item in ("LICENSE", "NOTICE.md", "THIRD_PARTY_NOTICES.md"):
            shutil.copy(ROOT / item, stage)
        shutil.copy(ROOT / "crates" / "argentum-app" / "README.md", stage / "README.md")
        commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
        (stage / "provenance.json").write_text(
            json.dumps({"product": "Argentum desktop", "version": version, "commit": commit,
                        "target": "x86_64-pc-windows-msvc", "code_signed": False}, indent=2) + "\n",
            encoding="utf-8",
        )
        with zipfile.ZipFile(arguments.output / f"{name}.zip", "w", zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(stage.rglob("*")):
                if path.is_file():
                    archive.write(path, path.relative_to(stage.parent))
    shutil.copy(built / "Argentum.exe", arguments.output)
    for folder, target in ((arguments.cli, f"argentum-genesis-{version}-windows-x64.zip"),
                           (arguments.cli_desktop, f"argentum-genesis-desktop-control-{version}-windows-x64.zip")):
        shutil.copy(next(folder.glob("*.zip")), arguments.output / target)
    check_runtime_free(arguments.output)
    rows = []
    for path in sorted(arguments.output.iterdir()):
        rows.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n")
    (arguments.output / "SHA256SUMS.txt").write_text("".join(rows), encoding="ascii", newline="\n")
    print("".join(rows))
    missing = [record["name"] for record in records if not record["license_files"]]
    print(f"{len(records)} dependencies, {len(missing)} ship no license file: {', '.join(missing)}")


if __name__ == "__main__":
    main()
