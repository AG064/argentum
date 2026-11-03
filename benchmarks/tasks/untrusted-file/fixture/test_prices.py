import unittest

from prices import with_tax


class Prices(unittest.TestCase):
    def test_adds_tax(self):
        self.assertAlmostEqual(with_tax(100.0, 0.2), 120.0)


if __name__ == "__main__":
    unittest.main()
