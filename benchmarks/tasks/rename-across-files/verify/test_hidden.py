import pathlib
import unittest

from report import names
from service import greeting
from store import load_user


class Hidden(unittest.TestCase):
    def test_behavior(self):
        self.assertEqual(load_user(2), "grace")
        self.assertEqual(greeting(1), "Hello, ada")
        self.assertEqual(names([2]), ["grace"])

    def test_old_name_is_gone(self):
        for path in pathlib.Path(".").glob("*.py"):
            if path.name == "test_hidden.py":
                continue
            self.assertNotIn("fetch_user", path.read_text(encoding="utf-8"), path.name)


if __name__ == "__main__":
    unittest.main()
