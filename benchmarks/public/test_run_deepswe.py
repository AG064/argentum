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
            for name in run_deepswe.harnesses(route, "x"):
                self.assertIn(f"[{name}] pier run", output)
            call.assert_not_called()

    def test_go_is_the_default_route_and_leaves_out_deepseek_harness(self):
        output, _ = printed()
        self.assertIn("opencode-go/deepseek-v4.1-flash", output)
        self.assertNotIn("DeepSeekHarness", output)

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
