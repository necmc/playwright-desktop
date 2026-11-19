from contextlib import contextmanager
import os
from pathlib import Path
import struct
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest
from uuid import uuid4

from playwright_desktop import AmbiguousMatchError, DesktopBusyError, DesktopTimeoutError
from playwright_desktop._runtime import Session
from playwright_desktop._sync import Desktop
from test_patch import FakePage


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
    def _desktop(self):
        page = FakePage()
        desktop = Desktop(Session(page))
        desktop.configure(java_home=self.java_home)
        try:
            yield page, desktop
        finally:
            desktop.close()

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
        return process.pid

    @staticmethod
    def _stop_form(process):
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)

    def test_window_requires_an_exact_title_and_uses_pid_to_resolve_duplicates(self):
        title = self._title()
        first = self._start_form(title, "Primero")
        second = self._start_form(title, "Segundo")

        for pid, expected in ((first, "Primero"), (second, "Segundo")):
            with self.subTest(pid=pid), self._desktop() as (_, desktop):
                field = desktop.window(title=title, pid=pid).get_by_role("textbox", name="Alias")
                self.assertEqual(field.input_value(), expected)

        with self._desktop() as (_, desktop):
            with self.assertRaises(DesktopTimeoutError):
                desktop.window(title=title.upper()).get_by_role("textbox", name="Alias").input_value(timeout=0)
            with self.assertRaises(DesktopTimeoutError):
                desktop.window(title=title, pid=os.getpid()).get_by_role("textbox", name="Alias").input_value(timeout=0)
            with self.assertRaises(AmbiguousMatchError):
                desktop.window(title=title).get_by_role("textbox", name="Alias").input_value()

    def test_duplicate_controls_are_rejected(self):
        title = self._title()
        with self._desktop() as (_, desktop):
            with desktop.expect_window(title=title):
                self._start_form(title, "Primero", "Segundo")

            with self.assertRaises(DesktopTimeoutError):
                desktop.get_by_role("textbox", name="Inexistente").input_value(timeout=0)
            with self.assertRaises(AmbiguousMatchError):
                desktop.get_by_role("textbox", name="Alias").fill("Otro valor")

    def test_capture_ignores_windows_that_were_already_open(self):
        title = self._title()
        existing = self._start_form(title, "Existente")
        with self._desktop() as (_, desktop):
            field = desktop.window(title=title, pid=existing).get_by_role("textbox", name="Alias")
            self.assertEqual(field.input_value(), "Existente")

        with self._desktop() as (_, desktop):
            with desktop.expect_window(title=title):
                self._start_form(title, "Nueva")
            self.assertEqual(desktop.get_by_role("textbox", name="Alias").input_value(), "Nueva")

    def test_capture_keeps_the_reservation_until_window_page_or_session_closes(self):
        for release_by in ("window", "page", "session"):
            with self.subTest(release_by=release_by), self._desktop() as (page, owner), self._desktop() as (_, other):
                first_title, second_title = self._title(), self._title()
                with owner.expect_window(title=first_title) as opened:
                    self._start_form(first_title, "Primero")

                window = opened.value
                self.assertEqual(window.get_by_role("textbox", name="Alias").input_value(), "Primero")
                with self.assertRaises(DesktopBusyError):
                    with other.expect_window(title=second_title):
                        self.fail("The second desktop cannot reserve the active window.")

                if release_by == "window":
                    window.get_by_role("button", name="Cerrar").click()
                    window.wait_for_closed()
                elif release_by == "page":
                    page.callbacks["close"]()
                else:
                    owner.close()

                with other.expect_window(title=second_title):
                    self._start_form(second_title, "Segundo")
                self.assertEqual(other.get_by_role("textbox", name="Alias").input_value(), "Segundo")


if __name__ == "__main__":
    unittest.main()
