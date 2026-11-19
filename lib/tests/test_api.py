import os
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from playwright.sync_api import TimeoutError as PlaywrightTimeoutError

from playwright_desktop._errors import DesktopConfigurationError, DesktopTimeoutError
from playwright_desktop._models import WindowQuery, validate_control, validate_timeout, validate_text
from playwright_desktop._runtime import Session
from playwright_desktop._sync import Desktop
from test_patch import FakePage


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

    def test_missing_java_home_is_reported_before_loading_the_engine(self):
        page = FakePage()
        desktop = Desktop(Session(page))
        with patch.dict(os.environ), patch("playwright_desktop._runtime._get_engine") as get_engine:
            os.environ.pop("JAVA_HOME", None)
            with self.assertRaisesRegex(DesktopConfigurationError, "JAVA_HOME"):
                with desktop.expect_window(title="Información del equipo"):
                    self.fail("Capture should require a runtime directory.")
        get_engine.assert_not_called()

    def test_text_limit_counts_utf16_units_and_rejects_surrogates(self):
        validate_text("\U00010400" * 511 + "A")
        validate_text("")
        for text in ("\U00010400" * 512, "A\0B", "\ud800"):
            with self.assertRaises(ValueError):
                validate_text(text)

    def test_desktop_actions_use_the_window_captured_without_an_event_variable(self):
        page = FakePage()
        session = Session(page)
        desktop = Desktop(session)
        with self.assertRaisesRegex(DesktopConfigurationError, "expect_window"):
            desktop.get_by_role("button", name="Confirmar")

        replies = [SimpleNamespace(handle=handle) for handle in (1, 2, 2, 2, 2)]
        with patch.object(session, "call", side_effect=replies) as request:
            with desktop.expect_window(title="Información del equipo"):
                pass
            desktop.get_by_role("textbox", name="Alias").fill("QA")
            desktop.get_by_role("button", name="Confirmar").click()
            desktop.wait_for_closed()

        self.assertEqual([call.args[0] for call in request.call_args_list],
                         ["prepare", "capture", "fill", "click", "wait_closed"])
        self.assertEqual([call.kwargs["window_id"] for call in request.call_args_list[2:]], [2, 2, 2])


if __name__ == "__main__":
    unittest.main()
