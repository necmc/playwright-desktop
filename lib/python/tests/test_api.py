import os
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from playwright.sync_api import TimeoutError as PlaywrightTimeoutError

import playwright_desktop
from playwright_desktop import _backend, _patch, _platform, _runtime
from playwright_desktop._errors import (
    DesktopConfigurationError,
    DesktopError,
    DesktopTimeoutError,
    UnsupportedPlatformError,
)
from playwright_desktop._models import WindowQuery, validate_control, validate_text, validate_timeout
from playwright_desktop._sync import Alert, App, DesktopContext, Window
from test_patch import FakeContext, FakePage


class ApiTests(unittest.TestCase):
    def setUp(self):
        playwright_desktop.install()

    def tearDown(self):
        _runtime.reset_defaults()

    def test_query_requires_a_title_or_a_positive_pid(self):
        self.assertEqual(WindowQuery("My window", 42).title, "My window")
        self.assertEqual(WindowQuery(None, 42).pid, 42)
        self.assertEqual(WindowQuery(None, 42, "Opciones").title_prefix, "Opciones")
        self.assertEqual(WindowQuery(has_text="Windows protegió su PC").has_text, "Windows protegió su PC")
        self.assertTrue(WindowQuery.capture().allow_empty)
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

    def test_automatic_app_capture_requires_no_java_home(self):
        page = FakePage()
        expect_app = _patch.ExpectAppMethod("sync").__get__(page, FakePage)
        with patch.dict(os.environ), patch("playwright_desktop._runtime._get_engine") as get_engine:
            os.environ.pop("JAVA_HOME", None)
            get_engine.return_value.invoke.side_effect = [
                SimpleNamespace(handle=1, pid=None, title=None, text=None),
                SimpleNamespace(handle=2, pid=404, title="Opciones", text=None),
            ]
            with expect_app(title="Opciones") as opened:
                self.assertIsInstance(opened.context, DesktopContext)
            app = opened.value
        self.assertIsInstance(app, App)
        self.assertEqual(app.pid, 404)
        self.assertEqual(app.title, "Opciones")
        get_engine.return_value.open_auto_session.assert_called_once_with(None, explicit=False)
        app.context.close()
        self.assertFalse(page.context.is_closed())

    def test_expect_app_returns_app_and_intermediate_apps_share_app_interface(self):
        context = FakeContext()
        page = FakePage(context=context)
        expect_app = _patch.ExpectAppMethod("sync").__get__(page, FakePage)
        session = _runtime.session_for_context(context)
        self.addCleanup(session.close_nowait)

        replies = [
            SimpleNamespace(handle=1, pid=None, title=None, text=None),
            SimpleNamespace(handle=9, pid=11520, title="", text=None),
            SimpleNamespace(handle=2, pid=8080, title="Información del equipo", text=None),
            SimpleNamespace(handle=2, pid=8080, title="Información del equipo", text=None),
            SimpleNamespace(handle=2, pid=8080, title="Información del equipo", text=None),
        ]
        with patch.object(session, "call", side_effect=replies) as request:
            with expect_app(title="Información del equipo") as opened:
                dcontext = opened.context
                intermediate = dcontext.app(has_text="Windows protegió su PC")
                self.assertIsInstance(intermediate, App)
                intermediate.get_by_role("link", name="Más información").click()

            app = opened.value
            self.assertIs(app.context, dcontext)
            self.assertIs(dcontext.browser_context, context)
            self.assertEqual(dcontext.apps, (app,))
            app.get_by_role("textbox", name="Alias").fill("QA")
            app.bring_to_front()

        self.assertEqual(
            [call.args[0] for call in request.call_args_list],
            ["prepare", "click", "capture", "fill", "bring_to_front"],
        )
        self.assertEqual(request.call_args_list[1].kwargs["has_text"], "Windows protegió su PC")
        self.assertEqual(request.call_args_list[4].kwargs["window_id"], 2)

    def test_app_close_and_window_and_alert_scope_without_closing_browser_context(self):
        page = FakePage()
        expect_app = _patch.ExpectAppMethod("sync").__get__(page, FakePage)
        session = _runtime.session_for_context(page.context)
        self.addCleanup(session.close_nowait)

        replies = [
            SimpleNamespace(handle=10, pid=None, title=None),
            SimpleNamespace(handle=20, pid=500, title="Principal"),
            SimpleNamespace(handle=21, pid=None, title=None),
            SimpleNamespace(handle=22, pid=500, title="Opciones"),
            SimpleNamespace(handle=23, pid=None, title=None),
            SimpleNamespace(handle=24, pid=500, title="Error"),
            SimpleNamespace(handle=24, pid=None, title=None),
            SimpleNamespace(handle=20, pid=None, title=None),
        ]
        with patch.object(session, "call", side_effect=replies) as request:
            with expect_app() as opened:
                pass
            app = opened.value
            with app.expect_window(title="Opciones") as win_info:
                pass
            win = win_info.value
            self.assertIsInstance(win, Window)
            self.assertIs(win.app, app)
            self.assertIs(win.context, app.context)

            with app.expect_alert(has_text="Campo requerido") as alert_info:
                pass
            alert = alert_info.value
            self.assertIsInstance(alert, Alert)
            self.assertIsInstance(alert, Window)
            alert.close()
            app.close()

        self.assertEqual(request.call_args_list[2].kwargs["pid"], 500)
        self.assertEqual(request.call_args_list[4].kwargs["pid"], 500)
        self.assertEqual(request.call_args_list[4].kwargs["has_text"], "Campo requerido")
        self.assertEqual(request.call_args_list[6].args[0], "close_window")
        self.assertEqual(request.call_args_list[6].kwargs["window_id"], 24)
        self.assertEqual(request.call_args_list[7].args[0], "close_window")
        self.assertEqual(request.call_args_list[7].kwargs["window_id"], 20)
        self.assertEqual(app.context.apps, ())
        self.assertFalse(page.context.is_closed())

    def test_expect_child_app_launches_separate_process_without_parent_pid(self):
        page = FakePage()
        expect_app = _patch.ExpectAppMethod("sync").__get__(page, FakePage)
        session = _runtime.session_for_context(page.context)
        self.addCleanup(session.close_nowait)
        replies = [
            SimpleNamespace(handle=1, pid=None, title=None),
            SimpleNamespace(handle=2, pid=100, title="Launcher"),
            SimpleNamespace(handle=3, pid=None, title=None),
            SimpleNamespace(handle=4, pid=200, title="Java App"),
        ]
        with patch.object(session, "call", side_effect=replies) as request:
            with expect_app(title="Launcher") as launcher_info:
                pass
            launcher = launcher_info.value
            with launcher.expect_child_app(title="Java App") as child_info:
                pass
            child = child_info.value

        self.assertIsInstance(child, App)
        self.assertIs(child.context, launcher.context)
        self.assertEqual(request.call_args_list[2].kwargs["pid"], None)
        self.assertEqual(launcher.context.apps, (launcher, child))

    def test_a_popup_scope_is_sent_with_the_action(self):
        page = FakePage()
        session = _runtime.session_for_context(page.context)
        self.addCleanup(session.close_nowait)
        dcontext = DesktopContext(session)
        for role in ("dialog", "pane"):
            with self.subTest(role=role), patch.object(
                session, "call", return_value=SimpleNamespace(handle=4)
            ) as request:
                pane = dcontext.app(title="Edge", pid=42).get_by_role(role, name="Descargas")
                link = pane.get_by_role("link", name="Abrir archivo")
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

    def test_expired_capture_cancels_the_reservation_without_waiting_again(self):
        page = FakePage()
        expect_app = _patch.ExpectAppMethod("sync").__get__(page, FakePage)
        session = _runtime.session_for_context(page.context)
        self.addCleanup(session.close_nowait)
        with (
            patch.object(session, "call", return_value=SimpleNamespace(handle=7)) as request,
            patch.object(session, "cancel_capture") as cancel,
            patch("playwright_desktop._timing.monotonic", side_effect=(1.0, 1.1)),
        ):
            with self.assertRaises(DesktopTimeoutError):
                with expect_app(title="Información del equipo", timeout=50):
                    pass

        request.assert_called_once_with(
            "prepare", title="Información del equipo", pid=None, title_prefix="", has_text="", only_new=False, timeout=50
        )
        cancel.assert_called_once_with(7)

    def test_predicate_rejection_raises_error_and_does_not_register_app(self):
        page = FakePage()
        expect_app = _patch.ExpectAppMethod("sync").__get__(page, FakePage)
        session = _runtime.session_for_context(page.context)
        self.addCleanup(session.close_nowait)
        replies = [
            SimpleNamespace(handle=1, pid=None, title=None),
            SimpleNamespace(handle=2, pid=99, title="Otra ventana"),
        ]
        with patch.object(session, "call", side_effect=replies):
            with self.assertRaises(DesktopError):
                with expect_app(predicate=lambda app: app.title == "Esperada") as opened:
                    pass
        self.assertEqual(opened.context.apps, ())

    def test_os_language_returns_bcp47_and_is_exposed_on_desktop_context(self):
        self.assertFalse(hasattr(_platform, "ctypes"))
        page = FakePage()
        session = _runtime.session_for_context(page.context)
        self.addCleanup(session.close_nowait)
        dcontext = DesktopContext(session)

        lang = playwright_desktop.os_language()
        self.assertIsInstance(lang, str)
        self.assertTrue(len(lang) >= 2)
        self.assertNotIn("_", lang)
        self.assertEqual(dcontext.os_language, lang)

        with patch.object(_backend, "os_language", return_value="fr-FR"):
            self.assertEqual(playwright_desktop.os_language(), "fr-FR")
            self.assertEqual(dcontext.os_language, "fr-FR")

        with (
            patch.object(_backend, "os_language", side_effect=DesktopError("missing")),
            patch.object(_platform.locale, "getlocale", return_value=("es_PE", "UTF-8")),
        ):
            self.assertEqual(playwright_desktop.os_language(), "es-PE")

        with (
            patch.object(_platform.sys, "platform", "linux"),
            patch.object(_platform.locale, "getlocale", return_value=("en_US", "UTF-8")),
        ):
            self.assertEqual(playwright_desktop.os_language(), "en-US")
            self.assertEqual(dcontext.os_language, "en-US")

    def test_app_only_new_validation_and_forwarding(self):
        with self.assertRaisesRegex(ValueError, "only_new"):
            WindowQuery(title="Aviso", only_new="yes")

        page = FakePage()
        session = _runtime.session_for_context(page.context)
        self.addCleanup(session.close_nowait)
        dcontext = DesktopContext(session)
        with patch.object(session, "call", return_value=SimpleNamespace(handle=8)) as request:
            dcontext.app(has_text="Windows protegió su PC", only_new=True).get_by_role("link", name="Más información").click()
        self.assertTrue(request.call_args.kwargs["only_new"])

        with patch.object(session, "call", return_value=SimpleNamespace(handle=9)) as default_request:
            dcontext.app(title="Edge", pid=42).get_by_role("button", name="Abrir").click()
        self.assertFalse(default_request.call_args.kwargs["only_new"])


if __name__ == "__main__":
    unittest.main()
