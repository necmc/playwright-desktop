"""Confirmación opcional del instalador ClickOnce de pc-info."""

from collections.abc import Callable
from contextlib import ExitStack
import ctypes
from locale import windows_locale
from urllib.parse import urlsplit

from playwright.sync_api import Page
from playwright_desktop import DesktopTimeoutError


class ClickOnceInstaller:
    def __init__(self, page: Page, *, application_name: str, base_url: str):
        self.page = page
        self.application_name = application_name
        self.source = urlsplit(base_url).hostname
        language = windows_locale.get(ctypes.windll.kernel32.GetUserDefaultUILanguage(), "")
        if language.startswith("es_"):
            self.title = "Ejecución de la aplicación - Advertencia de seguridad"
            self.run_button = "Ejecutar"
        else:
            self.title = "Application Run - Security Warning"
            self.run_button = "Run"

    def confirm_after(self, open_manifest: Callable[[], None]) -> None:
        with ExitStack() as capture:
            opened = capture.enter_context(
                self.page.desktop.expect_window(title=self.title, timeout=10_000)
            )
            open_manifest()
            # Solo el timeout al esperar el aviso permite continuar sin confirmarlo.
            try:
                capture.close()
            except DesktopTimeoutError:
                return

        notice = opened.value
        for label in (self.application_name, self.source):
            try:
                notice.get_by_role("text", name=label).wait_for(timeout=1_000)
            except DesktopTimeoutError:
                raise AssertionError(f"El aviso de ClickOnce no muestra «{label}».") from None

        notice.get_by_role("button", name=self.run_button).click(timeout=3_000)
        print(f"ClickOnce: clic en {self.run_button} para {self.application_name}.", flush=True)
