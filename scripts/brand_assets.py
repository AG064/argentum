#!/usr/bin/env python3
"""Regenerate or verify the checked-in Argentum PNG identity assets."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile

import PIL
from PIL import Image


EXPECTED_PILLOW_VERSION = "12.2.0"
EXPECTED_VARIANT_SIZES = (16, 20, 24, 32, 48, 64, 128, 256, 512)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def workspace_path(workspace: Path, relative_path: str) -> Path:
    candidate = (workspace / relative_path).resolve()
    try:
        candidate.relative_to(workspace)
    except ValueError as error:
        raise RuntimeError(f"Manifest path leaves the workspace: {relative_path}") from error
    return candidate


def load_manifest(workspace: Path) -> tuple[Path, dict]:
    manifest_path = workspace / "assets" / "brand" / "manifest.json"
    with manifest_path.open("r", encoding="utf-8") as stream:
        manifest = json.load(stream)
    if manifest.get("schemaVersion") != 1:
        raise RuntimeError("Unsupported brand manifest schema")
    return manifest_path, manifest


def verify_declared_sources(workspace: Path, manifest: dict) -> Image.Image:
    source = manifest["source"]
    active_path = workspace_path(workspace, source["activePath"])
    legacy_path = workspace_path(workspace, source["legacyPath"])
    expected_hash = source["sha256"]

    for path in (active_path, legacy_path):
        if not path.is_file():
            raise RuntimeError(f"Missing canonical source: {path}")
        actual_hash = sha256(path)
        if actual_hash != expected_hash:
            raise RuntimeError(
                f"Canonical source hash mismatch for {path}: {actual_hash}"
            )

    image = Image.open(active_path).convert("RGBA")
    expected_size = (source["width"], source["height"])
    if image.size != expected_size:
        raise RuntimeError(f"Canonical source size changed: {image.size}")

    alpha_bounds = image.getchannel("A").getbbox()
    declared_bounds = source["alphaBounds"]
    expected_bounds = (
        declared_bounds["x"],
        declared_bounds["y"],
        declared_bounds["x"] + declared_bounds["width"],
        declared_bounds["y"] + declared_bounds["height"],
    )
    if alpha_bounds != expected_bounds:
        raise RuntimeError(f"Canonical alpha bounds changed: {alpha_bounds}")
    return image


def verify_platform_assets(workspace: Path, manifest: dict) -> None:
    for asset in manifest["platformAssets"]:
        active_path = workspace_path(workspace, asset["path"])
        legacy_path = workspace_path(workspace, asset["legacyPath"])
        expected_hash = asset["sha256"]
        for path in (active_path, legacy_path):
            if not path.is_file():
                raise RuntimeError(f"Missing platform identity asset: {path}")
            actual_hash = sha256(path)
            if actual_hash != expected_hash:
                raise RuntimeError(
                    f"Platform identity hash mismatch for {path}: {actual_hash}"
                )


def render_variants(workspace: Path, manifest: dict, image: Image.Image) -> None:
    if PIL.__version__ != EXPECTED_PILLOW_VERSION:
        raise RuntimeError(
            "Writing identity assets requires Pillow "
            f"{EXPECTED_PILLOW_VERSION}, found {PIL.__version__}"
        )

    crop = manifest["derivation"]["crop"]
    crop_box = (
        crop["x"],
        crop["y"],
        crop["x"] + crop["width"],
        crop["y"] + crop["height"],
    )
    if crop["width"] != crop["height"]:
        raise RuntimeError("Brand crop must remain square")

    premultiplied = image.crop(crop_box).convert("RGBa")
    variants = {variant["size"]: variant for variant in manifest["variants"]}
    if tuple(sorted(variants)) != EXPECTED_VARIANT_SIZES:
        raise RuntimeError("Manifest variant sizes do not match the approved set")

    for size in EXPECTED_VARIANT_SIZES:
        destination = workspace_path(workspace, variants[size]["path"])
        if destination.parent != workspace / "assets" / "brand":
            raise RuntimeError(f"Variant destination is not in assets/brand: {destination}")
        rendered = premultiplied.resize(
            (size, size), Image.Resampling.LANCZOS
        ).convert("RGBA")
        with tempfile.NamedTemporaryFile(
            prefix=f".{destination.stem}.",
            suffix=".tmp.png",
            dir=destination.parent,
            delete=False,
        ) as temporary:
            temporary_path = Path(temporary.name)
        try:
            rendered.save(
                temporary_path,
                format="PNG",
                compress_level=9,
                optimize=False,
            )
            os.replace(temporary_path, destination)
        finally:
            temporary_path.unlink(missing_ok=True)


def verify_variants(workspace: Path, manifest: dict) -> None:
    declared_sizes = []
    for variant in manifest["variants"]:
        path = workspace_path(workspace, variant["path"])
        size = variant["size"]
        declared_sizes.append(size)
        if not path.is_file():
            raise RuntimeError(f"Missing brand variant: {path}")
        with Image.open(path) as image:
            if image.mode != "RGBA" or image.size != (size, size):
                raise RuntimeError(
                    f"Invalid brand variant geometry for {path}: "
                    f"{image.mode} {image.size}"
                )
        actual_hash = sha256(path)
        if actual_hash != variant["sha256"]:
            raise RuntimeError(f"Brand variant hash mismatch for {path}: {actual_hash}")
    if tuple(sorted(declared_sizes)) != EXPECTED_VARIANT_SIZES:
        raise RuntimeError("Manifest variant sizes do not match the approved set")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--write",
        action="store_true",
        help="Regenerate only the nine declared PNG variants before verification",
    )
    args = parser.parse_args()

    workspace = Path(__file__).resolve().parent.parent
    manifest_path, manifest = load_manifest(workspace)
    image = verify_declared_sources(workspace, manifest)
    verify_platform_assets(workspace, manifest)
    if args.write:
        render_variants(workspace, manifest, image)
    verify_variants(workspace, manifest)
    mode = "Regenerated and verified" if args.write else "Verified"
    print(f"{mode} Argentum identity assets from {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
