import unittest
from datetime import date

from dates import days_between


class Hidden(unittest.TestCase):
    def test_single_day(self):
        self.assertEqual(days_between(date(2026, 5, 1), date(2026, 5, 1)), [date(2026, 5, 1)])

    def test_across_a_month(self):
        days = days_between(date(2026, 1, 31), date(2026, 2, 2))
        self.assertEqual(len(days), 3)
        self.assertEqual(days[-1], date(2026, 2, 2))

    def test_end_before_start_is_empty(self):
        self.assertEqual(days_between(date(2026, 1, 2), date(2026, 1, 1)), [])


if __name__ == "__main__":
    unittest.main()
