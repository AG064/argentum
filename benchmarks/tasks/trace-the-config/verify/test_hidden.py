import pathlib
import unittest


class Hidden(unittest.TestCase):
    def test_answer(self):
        self.assertEqual(pathlib.Path("ANSWER.txt").read_text(encoding="utf-8").strip(), "75")


if __name__ == "__main__":
    unittest.main()
