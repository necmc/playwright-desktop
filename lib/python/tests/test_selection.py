from contextlib import contextmanager
import os
from pathlib import Path
from shutil import copyfile
import struct
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest
from uuid import uuid4

from playwright_desktop import (
    AmbiguousMatchError,
    DesktopBusyError,
    DesktopConfigurationError,
    DesktopContext,
    DesktopTimeoutError,
    StaleWindowError,
    _runtime,
)
from test_patch import FakeContext


@unittest.skipUnless(sys.platform == "win32" and struct.calcsize("P") == 8, "Requires Windows x64.")
class WindowSelectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        java_home = os.environ.get("PLAYWRIGHT_DESKTOP_TEST_JAVA_HOME")
        if not java_home:
            raise unittest.SkipTest("Set PLAYWRIGHT_DESKTOP_TEST_JAVA_HOME to run the Swing cases.")
        cls.java_home = Path(java_home).resolve(strict=True)
        directory = TemporaryDirectory(prefix="desktop-selection-")
        cls.addClassCleanup(directory.cleanup)
        cls.workdir = Path(directory.name)
        source = Path(__file__).with_name("fixtures") / "SelectionForm.java"
        subprocess.run(
            [cls.java_home / "bin/javac.exe", "--release", "11", "-d", cls.workdir, source],
            creationflags=subprocess.CREATE_NO_WINDOW,
            timeout=30,
            check=True,
        )

    @staticmethod
    def _title():
        return f"Playwright desktop selection {uuid4().hex}"

    @contextmanager
    def _desktop_context(self, *, java_home=None):
        context = FakeContext()
        session = _runtime.session_for_context(context)
        session.configure(java_home=self.java_home if java_home is None else java_home)
        dcontext = DesktopContext(session)
        try:
            yield context, dcontext
        finally:
            dcontext.close()

    def _start_form(self, title, *values):
        env = os.environ.copy()
        for key in ("JAVA_TOOL_OPTIONS", "JDK_JAVA_OPTIONS", "_JAVA_OPTIONS"):
            env.pop(key, None)
        process = subprocess.Popen(
            [self.java_home / "bin/java.exe", f"-Duser.home={self.workdir}", "-Djava.awt.headless=false",
             "-Djavax.accessibility.assistive_technologies=com.sun.java.accessibility.AccessBridge",
             "-cp", self.workdir, "SelectionForm", title, *values],
            env=env,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        self.addCleanup(self._stop_form, process)
        return process

    @staticmethod
    def _stop_form(process):
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)

    def test_app_requires_an_exact_title_and_uses_pid_to_resolve_duplicates(self):
        title = self._title()
        first = self._start_form(title, "Primero")
        second = self._start_form(title, "Segundo")

        for process, expected in ((first, "Primero"), (second, "Segundo")):
            with self.subTest(pid=process.pid), self._desktop_context() as (_, dcontext):
                field = dcontext.app(title=title, pid=process.pid).get_by_role("textbox", name="Alias")
                self.assertEqual(field.input_value(), expected)
                self.assertEqual(field.input_value(timeout=0), expected)

        with self._desktop_context() as (_, dcontext):
            with self.assertRaises(DesktopTimeoutError):
                dcontext.app(title=title.upper()).get_by_role("textbox", name="Alias").input_value(timeout=0)
            with self.assertRaises(DesktopTimeoutError):
                dcontext.app(title=title, pid=os.getpid()).get_by_role("textbox", name="Alias").input_value(timeout=0)
            with self.assertRaises(AmbiguousMatchError) as error:
                dcontext.app(title=title).get_by_role("textbox", name="Alias").input_value()
            message = str(error.exception)
            self.assertIn(f"pid={first.pid}", message)
            self.assertIn(f"pid={second.pid}", message)
            self.assertIn(title, message)

    def test_duplicate_controls_are_rejected(self):
        title = self._title()
        with self._desktop_context() as (_, dcontext):
            with dcontext.expect_app(title=title) as opened:
                self._start_form(title, "Primero", "Segundo")

            app = opened.value
            with self.assertRaises(DesktopTimeoutError):
                app.get_by_role("textbox", name="Inexistente").input_value(timeout=0)
            with self.assertRaises(AmbiguousMatchError):
                app.get_by_role("textbox", name="Alias").fill("Otro valor")

    def test_capture_ignores_windows_that_were_already_open(self):
        title = self._title()
        existing = self._start_form(title, "Existente")
        with self._desktop_context() as (_, dcontext):
            field = dcontext.app(title=title, pid=existing.pid).get_by_role("textbox", name="Alias")
            self.assertEqual(field.input_value(), "Existente")

        with self._desktop_context() as (_, dcontext):
            with dcontext.expect_app(title=title) as opened:
                self._start_form(title, "Nueva")
            app = opened.value
            self.assertEqual(app.get_by_role("textbox", name="Alias").input_value(), "Nueva")

    def test_capture_keeps_the_reservation_until_app_context_or_session_closes(self):
        for release_by in ("app", "context", "session"):
            with self.subTest(release_by=release_by), self._desktop_context() as (owner_ctx, owner), self._desktop_context() as (_, other):
                first_title, second_title = self._title(), self._title()
                with owner.expect_app(title=first_title) as opened:
                    self._start_form(first_title, "Primero")

                app = opened.value
                self.assertEqual(app.get_by_role("textbox", name="Alias").input_value(), "Primero")
                with self.assertRaises(DesktopBusyError):
                    with other.expect_app(title=second_title):
                        self.fail("The second desktop context cannot reserve the active window.")

                if release_by == "app":
                    app.get_by_role("button", name="Cerrar").click()
                    app.wait_for_closed()
                elif release_by == "context":
                    owner_ctx.close()
                else:
                    owner.close()

                with other.expect_app(title=second_title) as second_opened:
                    self._start_form(second_title, "Segundo")
                second_app = second_opened.value
                self.assertEqual(second_app.get_by_role("textbox", name="Alias").input_value(), "Segundo")

    def test_a_second_bridge_runtime_is_rejected_after_the_first_load(self):
        title = self._title()
        with self._desktop_context() as (_, dcontext):
            with dcontext.expect_app(title=title) as opened:
                self._start_form(title, "Primero")
            app = opened.value
            self.assertEqual(app.get_by_role("textbox", name="Alias").input_value(), "Primero")

        other_home = self.workdir / "second-runtime"
        bridge = Path("bin/windowsaccessbridge-64.dll")
        (other_home / bridge).parent.mkdir(parents=True, exist_ok=True)
        copyfile(self.java_home / bridge, other_home / bridge)
        with self._desktop_context(java_home=other_home) as (_, dcontext):
            with self.assertRaisesRegex(DesktopConfigurationError, "different Java bridge"):
                with dcontext.expect_app(title=self._title()):
                    self.fail("A second runtime must not replace the loaded bridge.")

    def test_a_window_closed_externally_is_stale_and_releases_the_engine(self):
        first_title, second_title = self._title(), self._title()
        with self._desktop_context() as (_, owner), self._desktop_context() as (_, other):
            with owner.expect_app(title=first_title) as opened:
                first = self._start_form(first_title, "Primero")

            app = opened.value
            self._stop_form(first)
            with self.assertRaises(StaleWindowError):
                app.get_by_role("textbox", name="Alias").input_value(timeout=0)
            app.wait_for_closed(timeout=0)

            with other.expect_app(title=second_title) as second_opened:
                self._start_form(second_title, "Segundo")
            second_app = second_opened.value
            self.assertEqual(second_app.get_by_role("textbox", name="Alias").input_value(), "Segundo")


if __name__ == "__main__":
    unittest.main()
