import unittest
from unittest.mock import patch

from playwright.sync_api import TimeoutError as PlaywrightTimeoutError

from playwright_desktop._errors import DesktopTimeoutError
from playwright_desktop._models import WindowQuery, validate_control, validate_timeout, validate_text


class ApiTests(unittest.TestCase):
    def test_query_requires_an_exact_nonempty_title_and_positive_pid(self):
        self.assertEqual(WindowQuery("My window", 42).title, "My window")
        for title, pid in (("", None), ("My window", 0), ("My window", True)):
            with self.assertRaises(ValueError):
                WindowQuery(title, pid)

    def test_only_supported_control_roles_and_nonempty_names_are_accepted(self):
        validate_control("textbox", "Alias")
        validate_control("button", "Confirmar")
        for role, name in (("tree", "x"), ("button", "")):
            with self.assertRaises(ValueError):
                validate_control(role, name)

    def test_timeout_validation_accepts_zero_and_rejects_nonfinite_values(self):
        self.assertEqual(validate_timeout(0), 0)
        self.assertEqual(validate_timeout(1.5), 1.5)
        for value in (-1, True, "100", float("nan"), float("inf")):
            with self.assertRaises(ValueError):
                validate_timeout(value)

    def test_native_timeout_can_be_caught_as_playwright_timeout(self):
        self.assertIsInstance(DesktopTimeoutError("timeout"), PlaywrightTimeoutError)

    def test_text_limit_counts_utf16_units_and_rejects_surrogates(self):
        validate_text("\U00010400" * 511 + "A")
        validate_text("")
        for text in ("\U00010400" * 512, "A\0B", "\ud800"):
            with self.assertRaises(ValueError):
                validate_text(text)


if __name__ == "__main__":
    unittest.main()
