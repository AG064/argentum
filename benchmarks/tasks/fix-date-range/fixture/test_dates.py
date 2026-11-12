import unittest
from datetime import date

from dates import days_between


class Days(unittest.TestCase):
    def test_includes_both_ends(self):
        days = days_between(date(2026, 1, 1), date(2026, 1, 3))
        self.assertEqual(days, [date(2026, 1, 1), date(2026, 1, 2), date(2026, 1, 3)])


if __name__ == "__main__":
    unittest.main()
