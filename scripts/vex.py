#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""Build the project's OpenVEX document from the reviewed advisories.

Each osv-scanner.toml lists advisories that were reviewed and found not to affect
the shipped apps, with the reason. This script turns those entries into
vex/argentum.openvex.json, so the same decisions are published in a standard
format. Run it after changing an osv-scanner.toml. With --check it fails if the
committed document is out of date, which CI runs on every pull request.
"""
import argparse
import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "vex" / "argentum.openvex.json"
SOURCES = [
    # (file, package ecosystem, what is shipped from it)
    ("osv-scanner.toml", "cargo", "desktop app, CLI and Android native library"),
    ("fuzz/osv-scanner.toml", "cargo", "fuzz targets, which are not shipped"),
    ("platforms/android/gradle/osv-scanner.toml", "maven", "Android build"),
]
PRODUCT = "pkg:github/AG064/argentum"


def purl(ecosystem: str, package: str) -> str:
    if ecosystem == "maven" and ":" in package:
        group, artifact = package.split(":", 1)
        return f"pkg:maven/{group}/{artifact}"
    return f"pkg:{ecosystem}/{package}"


def statements() -> list[dict]:
    found: dict[str, dict] = {}
    for name, ecosystem, _ in SOURCES:
        data = tomllib.loads((ROOT / name).read_text(encoding="utf-8"))
        for entry in data.get("IgnoredVulns", []):
            vuln, reason = entry["id"], entry["reason"].strip()
            # Each reason starts with the package it concerns, for example
            # "bincode: ..." or "io.netty:netty-common: ...".
            package, _, impact = reason.partition(": ")
            if not package or not impact:
                raise SystemExit(f"{name}: {vuln}: reason must start with '<package>: '")
            build_only = ecosystem == "maven" or name.startswith("fuzz/")
            statement = found.setdefault(vuln, {
                "vulnerability": {"name": vuln},
                "products": [{"@id": PRODUCT, "subcomponents": []}],
                "status": "not_affected",
                "justification": "component_not_present" if build_only else "vulnerable_code_not_present",
                "impact_statement": impact,
            })
            sub = {"@id": purl(ecosystem, package)}
            if sub not in statement["products"][0]["subcomponents"]:
                statement["products"][0]["subcomponents"].append(sub)
            if not build_only:
                # A shipped component decides the justification for the whole statement.
                statement["justification"] = "vulnerable_code_not_present"
                statement["impact_statement"] = impact
    return [found[key] for key in sorted(found)]


def document() -> dict:
    return {
        "@context": "https://openvex.dev/ns/v0.2.0",
        "@id": "https://github.com/AG064/argentum/blob/development/vex/argentum.openvex.json",
        "author": "AG064 <contact@ag064.eu>",
        "role": "Maintainer",
        "timestamp": "2026-11-25T00:00:00Z",
        "version": 1,
        "tooling": "scripts/vex.py",
        "statements": statements(),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail if the committed document is out of date")
    args = parser.parse_args()
    text = json.dumps(document(), indent=2, ensure_ascii=False) + "\n"
    if args.check:
        current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
        if current != text:
            print(f"{OUTPUT.relative_to(ROOT)} is out of date. Run: python scripts/vex.py", file=sys.stderr)
            return 1
        print(f"{OUTPUT.relative_to(ROOT)} matches the reviewed advisories")
        return 0
    OUTPUT.parent.mkdir(exist_ok=True)
    OUTPUT.write_text(text, encoding="utf-8", newline="\n")
    print(f"wrote {OUTPUT.relative_to(ROOT)} with {len(document()['statements'])} statements")
    return 0


if __name__ == "__main__":
    sys.exit(main())
