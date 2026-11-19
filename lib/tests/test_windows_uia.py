"""Interactive integration tests against controls hosted by a separate Windows process."""

import asyncio
import os
from pathlib import Path
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
from playwright_desktop import AmbiguousMatchError


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
        process = subprocess.Popen([self.application, self.title, self.results])
        self.addCleanup(self.stop, process)
        self.pid = process.pid
        self.wait_for_result("ready")

    @staticmethod
    def stop(process):
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)

    def wait_for_result(self, value):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            if self.results.exists() and value in self.results.read_text().splitlines():
                return
            time.sleep(0.02)
        self.fail(f"The Windows fixture did not report {value!r}.")

    def test_sync_click_opens_and_closes_a_new_dialog_without_java(self):
        with patch.dict(os.environ), sync_playwright() as playwright:
            os.environ.pop("JAVA_HOME", None)
            with playwright.chromium.launch(channel="msedge") as browser:
                page = browser.new_page()
                window = page.desktop.window(pid=self.pid)
                with page.desktop.expect_window(title=f"{self.title} - Opciones", pid=self.pid) as opened:
                    window.get_by_role("button", name="Mostrar opciones").click()
                dialog = opened.value
                dialog.get_by_role("button", name="Aceptar").click()
                dialog.wait_for_closed()
                self.wait_for_result("dialog")

    def test_async_click_is_scoped_to_the_popup_inside_the_existing_window(self):
        asyncio.run(self.verify_scoped_click())

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
                native = page.desktop.window(title=self.title, pid=self.pid)
                native.get_by_role("button", name="Mostrar aviso").click()
                with page.desktop.expect_window(title=java_title) as opened:
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
                    window = page.desktop.window(title=self.title, pid=self.pid)
                    await window.get_by_role("button", name="Mostrar aviso").click()
                    await asyncio.to_thread(self.wait_for_result, "notice")
                    with self.assertRaises(AmbiguousMatchError):
                        await window.get_by_role("button", name="Aplicar").click()
                    notice = window.get_by_role("group", name="Opciones integradas")
                    await notice.get_by_role("button", name="Aplicar").click()
                    await asyncio.to_thread(self.wait_for_result, "inside")
                    self.assertNotIn("outside", self.results.read_text().splitlines())
                finally:
                    await browser.close()
