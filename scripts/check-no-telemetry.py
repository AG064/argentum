#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""Fail if an analytics, telemetry or crash-reporting dependency or endpoint appears.

Argentum sends nothing about its users or their machines anywhere on its own.
SECURITY.md states the rule. This check keeps it true for every tracked file in
the active product (`legacy/` is archival and skipped):

- Rust packages in every Cargo.lock
- npm packages in every package.json and package-lock.json
- Gradle dependencies and plugins in every *.gradle and *.gradle.kts file
- known analytics, telemetry and crash-reporting hosts in any tracked text file

Run it from anywhere inside the repository. It exits with 1 and lists every
finding, or prints "no telemetry found" and exits with 0. `--root DIR` checks
another directory instead, which the tests use.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Files that must name the forbidden packages and hosts to check for them.
SELF = {"scripts/check-no-telemetry.py", "scripts/tests/test_check_no_telemetry.py"}
SKIPPED_PREFIXES = ("legacy/",)
MAX_TEXT_BYTES = 8 * 1024 * 1024

# Rust crates, matched against the whole name or a name prefix ending in "-" or "_".
RUST_CRATES = [
    "sentry",
    "opentelemetry",
    "tracing-opentelemetry",
    "posthog",
    "posthog-rs",
    "segment",
    "analytics-rs",
    "mixpanel",
    "amplitude",
    "datadog",
    "dogstatsd",
    "statsd",
    "cadence",
    "rudderanalytics",
    "newrelic",
    "bugsnag",
    "rollbar",
    "honeycomb",
    "libhoney",
    "appinsights",
    "applicationinsights",
    "prometheus",
    "metrics-exporter-prometheus",
    "metrics-exporter-statsd",
    "google-analytics",
    "ga4",
    "firebase",
    "aptabase",
    "countly",
    "crash-handler",
    "minidump-writer",
    "telemetry",
]

# npm packages, matched against the whole name, or a scope ending in "/".
NPM_PACKAGES = [
    "@sentry/",
    "posthog-js",
    "posthog-node",
    "analytics-node",
    "@segment/",
    "mixpanel",
    "mixpanel-browser",
    "@amplitude/",
    "amplitude-js",
    "dd-trace",
    "@datadog/",
    "@opentelemetry/",
    "newrelic",
    "@newrelic/",
    "bugsnag",
    "@bugsnag/",
    "rollbar",
    "applicationinsights",
    "@microsoft/applicationinsights-web",
    "react-ga",
    "react-ga4",
    "ga-4-react",
    "firebase",
    "@firebase/",
    "@vercel/analytics",
    "plausible-tracker",
    "@aptabase/",
    "countly-sdk-web",
    "universal-analytics",
]

# Maven group ids and Gradle plugin ids, matched anywhere in a Gradle file.
GRADLE_COORDINATES = [
    "com.google.firebase",
    "com.google.gms.google-services",
    "com.google.android.gms:play-services-analytics",
    "com.google.android.gms:play-services-measurement",
    "io.sentry",
    "com.microsoft.appcenter",
    "com.bugsnag",
    "com.crashlytics",
    "com.mixpanel",
    "com.amplitude",
    "com.segment",
    "com.posthog",
    "io.opentelemetry",
    "com.datadoghq",
    "com.newrelic",
    "com.microsoft.applicationinsights",
    "io.honeycomb",
    "ly.count.android",
]

# Hosts that only receive analytics, telemetry or crash reports.
ENDPOINT_HOSTS = [
    "sentry.io",
    "posthog.com",
    "segment.io",
    "segment.com",
    "mixpanel.com",
    "google-analytics.com",
    "googletagmanager.com",
    "app-measurement.com",
    "firebaselogging.googleapis.com",
    "firebase-settings.crashlytics.com",
    "crashlytics.com",
    "amplitude.com",
    "datadoghq.com",
    "datadoghq.eu",
    "appcenter.ms",
    "bugsnag.com",
    "rollbar.com",
    "newrelic.com",
    "nr-data.net",
    "dc.services.visualstudio.com",
    "applicationinsights.azure.com",
    "plausible.io",
    "aptabase.com",
    "honeycomb.io",
    "count.ly",
    "stats.g.doubleclick.net",
    "clarity.ms",
    "hotjar.com",
    "fullstory.com",
    "logrocket.com",
    "umami.is",
]

ENDPOINT_PATTERN = re.compile(
    r"(?<![A-Za-z0-9.-])(?:[A-Za-z0-9-]+\.)*(?:"
    + "|".join(re.escape(host) for host in ENDPOINT_HOSTS)
    + r")(?![A-Za-z0-9-])",
    re.IGNORECASE,
)
CARGO_NAME = re.compile(r'^name = "([^"]+)"\s*$', re.MULTILINE)


def tracked_files(root: Path) -> list[str]:
    try:
        output = subprocess.run(
            ["git", "-C", str(root), "ls-files", "-z"],
            check=True,
            capture_output=True,
        ).stdout
        names = [name for name in output.decode("utf-8").split("\0") if name]
    except (OSError, subprocess.CalledProcessError):
        names = [
            path.relative_to(root).as_posix()
            for path in root.rglob("*")
            if path.is_file() and ".git" not in path.relative_to(root).parts
        ]
    return sorted(
        name
        for name in names
        if name not in SELF and not name.startswith(SKIPPED_PREFIXES)
    )


def rust_crate_is_forbidden(name: str) -> bool:
    lowered = name.lower()
    if "telemetry" in lowered or "analytics" in lowered:
        return True
    return any(
        lowered == crate or lowered.startswith((crate + "-", crate + "_"))
        for crate in RUST_CRATES
    )


def npm_package_is_forbidden(name: str) -> bool:
    lowered = name.lower()
    if "telemetry" in lowered or "analytics" in lowered:
        return True
    return any(
        lowered.startswith(package) if package.endswith("/") else lowered == package
        for package in NPM_PACKAGES
    )


def check_cargo_lock(path: str, text: str) -> list[str]:
    return [
        f"{path}: Rust package '{name}' is an analytics or telemetry crate"
        for name in CARGO_NAME.findall(text)
        if rust_crate_is_forbidden(name)
    ]


def npm_names(path: str, text: str) -> list[str]:
    try:
        document = json.loads(text)
    except json.JSONDecodeError:
        return []
    if not isinstance(document, dict):
        return []
    names = []
    for section in (
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ):
        value = document.get(section)
        if isinstance(value, dict):
            names.extend(value)
    packages = document.get("packages")
    if path.endswith("package-lock.json") and isinstance(packages, dict):
        for key in packages:
            if "node_modules/" in key:
                names.append(key.rsplit("node_modules/", 1)[1])
    return names


def check_npm(path: str, text: str) -> list[str]:
    return [
        f"{path}: npm package '{name}' is an analytics or telemetry package"
        for name in sorted(set(npm_names(path, text)))
        if npm_package_is_forbidden(name)
    ]


def check_gradle(path: str, text: str) -> list[str]:
    lowered = text.lower()
    return [
        f"{path}: Gradle dependency or plugin '{coordinate}' is analytics or telemetry"
        for coordinate in GRADLE_COORDINATES
        if coordinate in lowered
    ]


def check_endpoints(path: str, text: str) -> list[str]:
    findings = []
    for number, line in enumerate(text.splitlines(), start=1):
        for match in ENDPOINT_PATTERN.finditer(line):
            findings.append(
                f"{path}:{number}: '{match.group(0)}' is an analytics or telemetry host"
            )
    return findings


def check(root: Path) -> list[str]:
    findings = []
    for name in tracked_files(root):
        path = root / name
        try:
            if path.stat().st_size > MAX_TEXT_BYTES:
                continue
            text = path.read_bytes().decode("utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        base = name.rsplit("/", 1)[-1]
        if base == "Cargo.lock":
            findings.extend(check_cargo_lock(name, text))
        if base in ("package.json", "package-lock.json"):
            findings.extend(check_npm(name, text))
        if base.endswith((".gradle", ".gradle.kts")) or base == "libs.versions.toml":
            findings.extend(check_gradle(name, text))
        findings.extend(check_endpoints(name, text))
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", type=Path, default=ROOT)
    arguments = parser.parse_args()
    findings = check(arguments.root.resolve())
    for finding in findings:
        print(finding)
    if findings:
        print(
            f"{len(findings)} telemetry finding(s). Argentum has no telemetry. See SECURITY.md.",
            file=sys.stderr,
        )
        return 1
    print("no telemetry found")
    return 0


if __name__ == "__main__":
    sys.exit(main())
