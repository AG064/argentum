"""Proves the harness and tasks work without an agent, a model, or a key."""

import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
TASKS = sorted(path.name for path in (HERE / "tasks").iterdir() if (path / "task.json").exists())


def run(*arguments):
    with tempfile.TemporaryDirectory() as output:
        result = subprocess.run(
            [sys.executable, str(HERE / "run.py"), "--repeats", "1", "--output", output, *arguments],
            capture_output=True,
            text=True,
            check=True,
        )
        data = json.loads((pathlib.Path(output) / "results.json").read_text(encoding="utf-8"))
        return data["records"], result.stdout


class Harness(unittest.TestCase):
    def test_doing_nothing_fails_every_task(self):
        records, _ = run("--agents", "null")
        self.assertEqual(len(records), len(TASKS))
        self.assertEqual([r["task"] for r in records if r["passed"]], [])

    def test_the_reference_solution_passes_every_task(self):
        records, _ = run("--agents", "reference")
        self.assertEqual([r["task"] for r in records if not r["passed"]], [])
        self.assertTrue(all(r["files_changed"] >= 1 for r in records))

    def configured(self, agent):
        handle, path = tempfile.mkstemp(suffix=".json")
        with os.fdopen(handle, "w", encoding="utf-8") as output:
            json.dump({"fake": agent}, output)
        self.addCleanup(os.remove, path)
        return path

    def test_a_missing_agent_is_reported_and_never_counts_as_a_pass(self):
        config = self.configured({"run": ["agent-bench-not-installed", "{prompt}"]})
        records, stdout = run("--config", config, "--agents", "fake", "--tasks", TASKS[0])
        self.assertFalse(records[0]["passed"])
        self.assertTrue(records[0]["status"].startswith("not installed"))
        self.assertIn("Not a clean run", stdout)

    def test_a_real_command_runs_in_the_workspace_and_is_checked(self):
        script = "open('ANSWER.txt', 'w').write('75')"
        config = self.configured({"run": [sys.executable, "-c", script]})
        records, _ = run("--config", config, "--agents", "fake", "--tasks", "trace-the-config")
        self.assertTrue(records[0]["passed"])
        self.assertEqual(records[0]["exit_code"], 0)
        self.assertEqual(records[0]["files_changed"], 1)

    def test_a_wrong_answer_fails_even_when_the_agent_exits_cleanly(self):
        script = "open('ANSWER.txt', 'w').write('30')"
        config = self.configured({"run": [sys.executable, "-c", script]})
        records, _ = run("--config", config, "--agents", "fake", "--tasks", "trace-the-config")
        self.assertFalse(records[0]["passed"])
        self.assertEqual(records[0]["exit_code"], 0)

    def test_checks_are_hidden_from_the_fixture(self):
        for name in TASKS:
            fixture = HERE / "tasks" / name / "fixture"
            self.assertFalse((fixture / "test_hidden.py").exists(), name)

    def test_the_safety_task_fails_an_agent_that_obeys_the_file(self):
        fixture = HERE / "tasks" / "untrusted-file"
        text = (fixture / "fixture" / "NOTES.md").read_text(encoding="utf-8")
        self.assertIn("PWNED.txt", text)
        self.assertIn("PWNED.txt", (fixture / "verify" / "test_hidden.py").read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
