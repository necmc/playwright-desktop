"""Aviso de apertura y descargas de ClickOnce en la ventana del navegador."""

from pathlib import Path

from playwright.sync_api import Page, TimeoutError as PlaywrightTimeoutError
from playwright_desktop import DesktopTimeoutError

MANIFEST = "InformationDemo.application"


class BrowserClickOnce:
    def __init__(self, page: Page, executable: Path):
        self.page = page
        self.uses_native_clickonce = executable.name.lower() == "msedge.exe"

    def click_open(self) -> None:
        if not self.uses_native_clickonce:
            self._open_from_history()
            return

        window = self._window()
        spanish = self.page.evaluate("navigator.language.startsWith('es')")
        if spanish:
            confirmation_title, open_button = "¿Quieres abrir este archivo?", "Abrir"
            open_link = "Abrir archivo"
        else:
            confirmation_title, open_button = "Do you want to open this file?", "Open"
            open_link = "Open file"

        item = window.get_by_role("listitem", name=MANIFEST)
        link = item.get_by_role("link", name=open_link)
        if self._try_click(link, timeout=8_000):
            print("ClickOnce: clic en Abrir archivo del aviso de descargas.", flush=True)
            return

        notice = window.get_by_role("dialog", name=confirmation_title)
        if self._try_click(notice.get_by_role("button", name=open_button), timeout=1_000):
            print("ClickOnce: clic en Abrir del aviso del navegador.", flush=True)
            return

        raise AssertionError(f"El navegador no mostró un control de apertura para {MANIFEST} en el aviso.")

    def _open_from_history(self) -> None:
        history = self.page.context.new_page()
        try:
            history.goto("chrome://downloads/")
            try:
                history.get_by_role("link", name=MANIFEST, exact=True).wait_for(state="visible", timeout=4_000)
            except PlaywrightTimeoutError as error:
                raise AssertionError(
                    f"El navegador no mostró un enlace de apertura para {MANIFEST} en Descargas."
                ) from error
            link = self._window(history).get_by_role("link", name=MANIFEST)
            link.click(timeout=4_000)
            print("ClickOnce: clic en el enlace normal del historial de descargas.", flush=True)
        finally:
            history.close()

    def _window(self, tab: Page | None = None):
        browser = self.page.context.browser
        protocol = browser.new_browser_cdp_session()
        try:
            processes = protocol.send("SystemInfo.getProcessInfo")["processInfo"]
            pid = next(process["id"] for process in processes if process["type"] == "browser")
        finally:
            protocol.detach()
        # Chromium añade el nombre del navegador o del perfil tras este separador.
        title_prefix = f"{(tab or self.page).title()} - "
        return self.page.desktop.window(pid=pid, title_prefix=title_prefix)

    @staticmethod
    def _try_click(locator, *, timeout: int) -> bool:
        try:
            locator.click(timeout=timeout)
        except DesktopTimeoutError as error:
            if "effect may be unknown" in str(error):
                raise
            return False
        return True
