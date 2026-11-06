"""Tests for make_app_icon's fit rule. Run: python3 -I scripts/test_make_app_icon.py"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from make_app_icon import fit  # noqa: E402


class FitTests(unittest.TestCase):
    def test_the_3d_cyber_logo_lands_centred_inside_the_body(self):
        # Its visible area today: x 10..1014, y 32..1024 (touching the bottom edge).
        w, h, x, y = fit((10, 32, 1014, 1024), canvas=1024, body=824)
        self.assertEqual((w, h), (824, 814))
        self.assertEqual((x, y), (100, 105))

    def test_a_square_fills_the_body_exactly(self):
        self.assertEqual(fit((0, 0, 1024, 1024), canvas=1024, body=824), (824, 824, 100, 100))

    def test_no_margin_fills_the_canvas(self):
        # The toolbar mark: trimmed and centred, no margin.
        w, h, x, y = fit((10, 32, 1014, 1024), canvas=64, body=64)
        self.assertEqual((w, h), (64, 63))
        self.assertEqual((x, y), (0, 0))

    def test_a_tall_image_is_bounded_by_its_height(self):
        w, h, x, y = fit((0, 0, 100, 400), canvas=1024, body=824)
        self.assertEqual((w, h), (206, 824))
        self.assertEqual((x, y), (409, 100))


if __name__ == "__main__":
    unittest.main()
