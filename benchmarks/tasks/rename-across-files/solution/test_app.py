import unittest

from report import names
from service import greeting
from store import load_user


class App(unittest.TestCase):
    def test_all(self):
        self.assertEqual(load_user(1), "ada")
        self.assertEqual(greeting(2), "Hello, grace")
        self.assertEqual(names([1, 2]), ["ada", "grace"])


if __name__ == "__main__":
    unittest.main()
