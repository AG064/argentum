# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 AG064

"""Check the DeepSWE commands without Pier, a model, or a key."""

import contextlib
import io
import os
import pathlib
import sys
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import run_deepswe  # noqa: E402

SECRET = "not-a-real-key"


def printed(*extra):
    output = io.StringIO()
    keys = {"DEEPSEEK_API_KEY": SECRET, "OPENCODE_API_KEY": SECRET}
    with mock.patch.dict(os.environ, keys), contextlib.redirect_stdout(output):
        with mock.patch.object(run_deepswe.subprocess, "call") as call:
            assert run_deepswe.main(["--tasks", "tasks", *extra]) == 0
    return output.getvalue(), call


class Commands(unittest.TestCase):
    def test_every_harness_is_listed_and_nothing_runs_by_default(self):
        for route in run_deepswe.ROUTES:
            output, call = printed("--route", route)
            for name in set(run_deepswe.harnesses(route, "x")) - run_deepswe.NEEDS_LOG_UPLOAD_OFF:
                self.assertIn(f"[{name}] pier run", output)
            call.assert_not_called()

    def test_go_is_the_default_route_and_leaves_out_deepseek_harness(self):
        output, _ = printed()
        self.assertIn("opencode-go/deepseek-v4.1-flash", output)
        self.assertNotIn("DeepSeekHarness", output)

    def test_every_harness_asks_for_the_same_effort(self):
        for route in run_deepswe.ROUTES:
            output, _ = printed("--route", route)
            for line in output.splitlines():
                if " pier run " in line and "Argentum" not in line and ":Pi " not in line:
                    self.assertTrue("reasoning_effort=max" in line or "variant=max" in line, line)

    def test_a_harness_that_uploads_session_logs_stays_out(self):
        output, _ = printed("--route", "deepseek")
        self.assertNotIn("DeepSeekHarness", output)
        with self.assertRaises(SystemExit):
            printed("--route", "deepseek", "--agents", "deepseek-harness")

    def test_a_key_never_reaches_a_command_or_the_output(self):
        output, call = printed("--go", "--agents", "claude-code", "codex")
        self.assertNotIn(SECRET, output)
        self.assertEqual(call.call_count, 2)
        for args in call.call_args_list:
            self.assertNotIn(SECRET, " ".join(args.args[0]))

    def test_every_agent_gets_the_same_tasks(self):
        output, _ = printed("--n-tasks", "7", "--seed", "3")
        lines = [line for line in output.splitlines() if " pier run " in line]
        self.assertEqual(len(lines), len(run_deepswe.harnesses("opencode-go", "x")))
        for line in lines:
            self.assertIn("--n-tasks 7 --sample-seed 3", line)

    def test_an_unknown_agent_stops_it(self):
        with self.assertRaises(SystemExit):
            printed("--agents", "nobody")


if __name__ == "__main__":
    unittest.main()
