import os
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from playwright.sync_api import TimeoutError as PlaywrightTimeoutError

from playwright_desktop import _backend, _platform
from playwright_desktop._errors import DesktopConfigurationError, DesktopTimeoutError, UnsupportedPlatformError
from playwright_desktop._models import WindowQuery, validate_control, validate_timeout, validate_text
from playwright_desktop._runtime import Session
from playwright_desktop._sync import Desktop
from test_patch import FakePage


class ApiTests(unittest.TestCase):
    def test_query_requires_a_title_or_a_positive_pid(self):
        self.assertEqual(WindowQuery("My window", 42).title, "My window")
        self.assertEqual(WindowQuery(None, 42).pid, 42)
        self.assertEqual(WindowQuery(None, 42, "Opciones").title_prefix, "Opciones")
        self.assertEqual(WindowQuery(has_text="Windows protegió su PC").has_text, "Windows protegió su PC")
        for title, pid in ((None, None), ("", 42), ("My window", 0), ("My window", True)):
            with self.assertRaises(ValueError):
                WindowQuery(title, pid)
        with self.assertRaisesRegex(ValueError, "title or title_prefix"):
            WindowQuery("Opciones", 42, "Opciones")
        with self.assertRaisesRegex(ValueError, "title_prefix"):
            WindowQuery(None, 42, "")
        with self.assertRaisesRegex(ValueError, "has_text"):
            WindowQuery(has_text="")

    def test_only_supported_control_roles_and_nonempty_names_are_accepted(self):
        validate_control("textbox", "Alias")
        validate_control("button", "Confirmar")
        for role, name in (("tree", "x"), ("button", "")):
            with self.assertRaises(ValueError):
                validate_control(role, name)

    def test_timeout_validation_accepts_zero_and_rejects_nonfinite_values(self):
        self.assertEqual(validate_timeout(0), 0)
        self.assertEqual(validate_timeout(1.5), 1.5)
        for value in (-1, True, "100", float("nan"), float("inf"), 10**1000):
            with self.assertRaises(ValueError):
                validate_timeout(value)

    def test_native_timeout_can_be_caught_as_playwright_timeout(self):
        self.assertIsInstance(DesktopTimeoutError("timeout"), PlaywrightTimeoutError)

    def test_unsupported_platforms_do_not_load_the_native_extension(self):
        targets = (("linux", "linux-x86_64"), ("darwin", "macosx-14.0-arm64"), ("win32", "win-arm64"))
        for platform, build_target in targets:
            with (
                self.subTest(target=build_target),
                patch.object(_platform.sys, "platform", platform),
                patch.object(_platform.sysconfig, "get_platform", return_value=build_target),
                patch.object(_backend, "import_module") as load_extension,
            ):
                with self.assertRaises(UnsupportedPlatformError):
                    _backend.load_extension()
                load_extension.assert_not_called()

    def test_automatic_window_capture_requires_no_java_home(self):
        page = FakePage()
        desktop = Desktop(Session(page))
        with patch.dict(os.environ), patch("playwright_desktop._runtime._get_engine") as get_engine:
            os.environ.pop("JAVA_HOME", None)
            with desktop.expect_window(title="Opciones"):
                pass
        get_engine.return_value.open_auto_session.assert_called_once_with(None, explicit=False)
        desktop.close()

    def test_a_popup_scope_is_sent_with_the_action(self):
        page = FakePage()
        session = Session(page)
        desktop = Desktop(session)
        for role in ("dialog", "pane"):
            with self.subTest(role=role), patch.object(
                session, "call", return_value=SimpleNamespace(handle=4)
            ) as request:
                popup = desktop.window(title="Edge", pid=42).get_by_role(role, name="Descargas")
                link = popup.get_by_role("link", name="Abrir archivo")
                link.wait_for(timeout=500)
                self.assertEqual(request.call_args.args[0], "wait_visible")
                self.assertEqual(request.call_args.kwargs["scope_role"], role)
                link.click()
                self.assertEqual(request.call_args.kwargs["scope_role"], role)
                self.assertEqual(request.call_args.kwargs["scope_name"], "Descargas")
                self.assertEqual(request.call_args.kwargs["role"], "link")

    def test_common_text_validation_accepts_long_unicode_and_rejects_invalid_text(self):
        validate_text("\U00010400" * 512)
        validate_text("")
        for text in ("A\0B", "\ud800"):
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

    def test_expired_capture_cancels_the_reservation_without_waiting_again(self):
        page = FakePage()
        session = Session(page)
        desktop = Desktop(session)
        with (
            patch.object(session, "call", return_value=SimpleNamespace(handle=7)) as request,
            patch.object(session, "cancel_capture") as cancel,
            patch("playwright_desktop._timing.monotonic", side_effect=(1.0, 1.1)),
        ):
            with self.assertRaises(DesktopTimeoutError):
                with desktop.expect_window(title="Información del equipo", timeout=50):
                    pass

        request.assert_called_once_with("prepare", title="Información del equipo", pid=None, timeout=50)
        cancel.assert_called_once_with(7)

    def test_window_has_text_forwards_the_visible_text_filter(self):
        page = FakePage()
        session = Session(page)
        desktop = Desktop(session)
        with patch.object(session, "call", return_value=SimpleNamespace(handle=9)) as request:
            desktop.window(has_text="Windows protegió su PC").get_by_role("link", name="Más información").click()
        self.assertEqual(request.call_args.kwargs["has_text"], "Windows protegió su PC")
        self.assertEqual(request.call_args.kwargs["title"], "")
        self.assertEqual(request.call_args.kwargs["title_prefix"], "")


if __name__ == "__main__":
    unittest.main()
