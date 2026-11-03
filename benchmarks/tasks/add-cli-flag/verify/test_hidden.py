import json
import os
import subprocess
import sys
import tempfile
import unittest


def run(*args):
    return subprocess.run(
        [sys.executable, "wordcount.py", *args], capture_output=True, text=True, check=True
    ).stdout


class Hidden(unittest.TestCase):
    def setUp(self):
        handle, self.path = tempfile.mkstemp()
        with os.fdopen(handle, "w", encoding="utf-8", newline="") as out:
            out.write("one two\nthree\n")
        self.addCleanup(os.remove, self.path)

    def test_plain_output_is_unchanged(self):
        self.assertEqual(run(self.path).strip(), "2 3 14")

    def test_json_output(self):
        self.assertEqual(json.loads(run("--json", self.path)), {"lines": 2, "words": 3, "chars": 14})

    def test_flag_after_path(self):
        self.assertEqual(json.loads(run(self.path, "--json"))["words"], 3)


if __name__ == "__main__":
    unittest.main()
