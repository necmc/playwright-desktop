"""Interactive integration tests against controls hosted by a separate Windows process."""

import asyncio
import os
from pathlib import Path
from shutil import copyfile
import subprocess
import sys
from tempfile import TemporaryDirectory
import time
import unittest
from unittest.mock import patch
from uuid import uuid4

from playwright.async_api import async_playwright
from playwright.sync_api import sync_playwright

import playwright_desktop
from playwright_desktop import AmbiguousMatchError, _runtime


UIA_TESTS = sys.platform == "win32" and os.environ.get("PLAYWRIGHT_DESKTOP_TEST_WINDOWS_UIA") == "1"


@unittest.skipUnless(UIA_TESTS, "Run in an interactive Windows session with PLAYWRIGHT_DESKTOP_TEST_WINDOWS_UIA=1.")
class WindowsUiaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        workspace = TemporaryDirectory(prefix="playwright-desktop-uia-")
        cls.addClassCleanup(workspace.cleanup)
        cls.directory = Path(workspace.name)
        cls.application = cls.directory / "NativeControlWindow.exe"
        framework = Path(os.environ["WINDIR"]) / "Microsoft.NET/Framework64/v4.0.30319"
        source = Path(__file__).with_name("fixtures") / "NativeControlWindow.cs"
        subprocess.run(
            [framework / "csc.exe", "/nologo", "/target:winexe", "/platform:x64",
             "/reference:System.Windows.Forms.dll", "/reference:System.Drawing.dll",
             f"/out:{cls.application}", source],
            check=True, timeout=30,
        )

    def setUp(self):
        self.title = f"Playwright desktop UIA {uuid4().hex}"
        self.results = self.directory / f"{uuid4().hex}.txt"

    def tearDown(self):
        _runtime.reset_defaults()

    def start_window(self, executable, results=None):
        target_results = self.results if results is None else results
        process = subprocess.Popen([executable, self.title, target_results])
        self.addCleanup(self.stop, process)
        self.wait_for_result("ready", results=target_results)
        return process.pid

    @staticmethod
    def stop(process):
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)

    def wait_for_result(self, value, results=None):
        target_results = self.results if results is None else results
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if target_results.exists() and value in target_results.read_text().splitlines():
                return
            time.sleep(0.02)
        self.fail(f"The Windows fixture did not report {value!r}.")

    def test_only_new_ignores_window_already_open_before_desktop_context_started(self):
        stale_results = self.directory / f"{uuid4().hex}-stale.txt"
        pid1 = self.start_window(self.application, results=stale_results)
        with patch.dict(os.environ), sync_playwright() as playwright:
            os.environ.pop("JAVA_HOME", None)
            with playwright.chromium.launch(channel="msedge") as browser:
                page = browser.new_page()
                with page.expect_app(title=self.title) as opened:
                    pid2 = self.start_window(self.application)
                    app2 = opened.context.app(title=self.title, only_new=True)
                    app2.get_by_role("button", name="Mostrar aviso").click()
                    self.wait_for_result("notice")
                captured = opened.value
                self.assertEqual(captured.pid, pid2)
                self.assertNotEqual(captured.pid, pid1)
                self.assertNotIn("notice", stale_results.read_text().splitlines())
                captured.close()
                self.assertFalse(page.context.is_closed())

    def test_sync_click_opens_and_closes_a_new_dialog_without_java(self):
        with patch.dict(os.environ), sync_playwright() as playwright:
            os.environ.pop("JAVA_HOME", None)
            with playwright.chromium.launch(channel="msedge") as browser:
                page = browser.new_page()
                with page.expect_app(title=self.title) as app_opened:
                    self.start_window(self.application)
                app = app_opened.value
                with app.expect_window(title=f"{self.title} - Opciones") as opened:
                    app.get_by_role("button", name="Mostrar opciones").click()
                dialog = opened.value
                dialog.get_by_role("button", name="Aceptar").click()
                dialog.wait_for_closed()
                self.wait_for_result("dialog")
                app.close()
                self.assertFalse(page.context.is_closed())

    def test_app_bring_to_front_and_close_preserve_parent_browser_context(self):
        with patch.dict(os.environ), sync_playwright() as playwright:
            os.environ.pop("JAVA_HOME", None)
            with playwright.chromium.launch(channel="msedge") as browser:
                page = browser.new_page()
                with page.expect_app(title=self.title) as app_opened:
                    pid = self.start_window(self.application)
                app = app_opened.value
                self.assertEqual(app.pid, pid)
                self.assertEqual(app.title, self.title)
                app.bring_to_front()
                app.close()
                app.wait_for_closed(timeout=0)
                self.assertEqual(app.context.apps, ())
                self.assertFalse(page.context.is_closed())
                self.assertFalse(page.is_closed())

    def test_async_click_is_scoped_to_the_panel_inside_the_existing_window(self):
        asyncio.run(self.verify_scoped_click())

    def test_java_named_windows_use_uia_when_bridge_does_not_recognize_them(self):
        renamed = self.directory / "java.exe"
        copyfile(self.application, renamed)
        with sync_playwright() as playwright, playwright.chromium.launch(channel="msedge") as browser:
            page = browser.new_page()
            with page.expect_app(title=self.title) as opened:
                self.start_window(renamed)
            app = opened.value
            app.get_by_role("button", name="Mostrar aviso").click(timeout=3000)
            self.wait_for_result("notice")

    def test_one_page_uses_native_windows_and_swing_automatically(self):
        java_home = os.environ.get("PLAYWRIGHT_DESKTOP_TEST_JAVA_HOME")
        if not java_home:
            self.skipTest("Set PLAYWRIGHT_DESKTOP_TEST_JAVA_HOME for the mixed UIA and Swing case.")
        java_bin = Path(java_home) / "bin"
        fixture = Path(__file__).with_name("fixtures") / "SelectionForm.java"
        subprocess.run(
            [java_bin / "javac.exe", "--release", "11", "-d", self.directory, fixture],
            check=True, timeout=30, creationflags=subprocess.CREATE_NO_WINDOW,
        )
        java_title = f"Playwright desktop Swing {uuid4().hex}"
        with patch.dict(os.environ, {"JAVA_HOME": java_home}), sync_playwright() as playwright:
            with playwright.chromium.launch(channel="msedge") as browser:
                page = browser.new_page()
                with page.expect_app(title=self.title) as native_opened:
                    self.start_window(self.application)
                native = native_opened.value
                native.get_by_role("button", name="Mostrar aviso").click()
                with native.expect_child_app(title=java_title) as opened:
                    java = subprocess.Popen(
                        [java_bin / "java.exe", f"-Duser.home={self.directory}",
                         "-Djavax.accessibility.assistive_technologies=com.sun.java.accessibility.AccessBridge",
                         "-cp", self.directory, "SelectionForm", java_title, "Swing"],
                        creationflags=subprocess.CREATE_NO_WINDOW,
                    )
                    self.addCleanup(self.stop, java)
                swing = opened.value
                alias = swing.get_by_role("textbox", name="Alias")
                self.assertEqual(alias.input_value(), "Swing")
                alias.fill("Swing confirmado")
                self.assertEqual(alias.input_value(), "Swing confirmado")
                with self.assertRaisesRegex(ValueError, "1023 UTF-16"):
                    alias.fill("\U00010400" * 512)
                self.assertEqual(alias.input_value(), "Swing confirmado")
                swing.get_by_role("button", name="Cerrar").click()
                swing.wait_for_closed()
                native.get_by_role("group", name="Opciones integradas").get_by_role("button", name="Aplicar").click()
                self.wait_for_result("inside")

    async def verify_scoped_click(self):
        with patch.dict(os.environ, {"JAVA_HOME": str(self.directory / "missing-java")}):
            async with async_playwright() as playwright:
                browser = await playwright.chromium.launch(channel="msedge")
                try:
                    page = await browser.new_page()
                    async with page.expect_app(title_prefix="Playwright desktop UIA ") as opened:
                        pid = self.start_window(self.application)
                    app = await opened.value
                    self.assertEqual(app.pid, pid)
                    await app.get_by_role("button", name="Mostrar aviso").click()
                    await asyncio.to_thread(self.wait_for_result, "notice")
                    with self.assertRaises(AmbiguousMatchError):
                        await app.get_by_role("button", name="Aplicar").click()
                    notice = app.get_by_role("group", name="Opciones integradas")
                    apply = notice.get_by_role("button", name="Aplicar")
                    await apply.wait_for()
                    await apply.click()
                    await asyncio.to_thread(self.wait_for_result, "inside")
                    self.assertNotIn("outside", self.results.read_text().splitlines())
                finally:
                    await browser.close()
