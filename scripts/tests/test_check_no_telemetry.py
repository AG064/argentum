#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064
"""Tests for scripts/check-no-telemetry.py. Run: python3 scripts/tests/test_check_no_telemetry.py"""
import importlib.util
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "check-no-telemetry.py"
spec = importlib.util.spec_from_file_location("check_no_telemetry", SCRIPT)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


class CheckNoTelemetry(unittest.TestCase):
    def run_on(self, files: dict[str, str]) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, text in files.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text, encoding="utf-8")
            return checker.check(root)

    def test_a_clean_tree_passes(self):
        findings = self.run_on(
            {
                "Cargo.lock": 'name = "serde"\nname = "unicode-segmentation"\nname = "icu_segmenter"\n',
                "src/main.rs": "// ScrollBar, telemetry is off, see huggingface.co\n",
                "extensions/browser/package.json": '{"dependencies": {"playwright": "1"}}',
            }
        )
        self.assertEqual(findings, [])

    def test_rust_crates_are_found(self):
        findings = self.run_on(
            {
                "Cargo.lock": 'name = "sentry-core"\nname = "opentelemetry_sdk"\nname = "my-telemetry"\nname = "segment"\n',
            }
        )
        self.assertEqual(len(findings), 4, findings)

    def test_npm_and_gradle_dependencies_are_found(self):
        findings = self.run_on(
            {
                "web/package.json": '{"devDependencies": {"@sentry/browser": "8", "posthog-js": "1"}}',
                "web/package-lock.json": '{"packages": {"node_modules/@amplitude/analytics-browser": {}}}',
                "android/app/build.gradle.kts": 'implementation("com.google.firebase:firebase-analytics:22")\n',
            }
        )
        self.assertEqual(len(findings), 4, findings)

    def test_endpoints_are_found_with_their_line(self):
        findings = self.run_on(
            {"src/lib.rs": 'fn a() {}\nconst URL: &str = "https://o1.ingest.sentry.io/api/1";\n'}
        )
        self.assertEqual(len(findings), 1, findings)
        self.assertIn("src/lib.rs:2:", findings[0])

    def test_legacy_is_skipped(self):
        self.assertEqual(self.run_on({"legacy/app.js": "https://api.segment.io/v1"}), [])


if __name__ == "__main__":
    unittest.main()
