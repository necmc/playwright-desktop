"""Aviso de apertura y descargas de ClickOnce en la ventana del navegador."""

from playwright.sync_api import Page
from playwright_desktop import DesktopContext, DesktopTimeoutError

MANIFEST = "InformationDemo.application"


class BrowserClickOnce:
    def __init__(self, dcontext: DesktopContext, page: Page):
        self.dcontext = dcontext
        self.page = page

    def click_open(self) -> None:
        browser_app = self._app()
        spanish = self.page.evaluate("navigator.language.startsWith('es')")
        if spanish:
            confirmation_title, open_button = "¿Quieres abrir este archivo?", "Abrir"
            open_link = "Abrir archivo"
        else:
            confirmation_title, open_button = "Do you want to open this file?", "Open"
            open_link = "Open file"

        notice = browser_app.get_by_role("pane", name=confirmation_title)
        if self._click_if_visible(notice.get_by_role("button", name=open_button), timeout=8_000):
            print("ClickOnce: clic en Abrir del aviso del navegador.", flush=True)
            return

        item = browser_app.get_by_role("listitem", name=MANIFEST)
        link = item.get_by_role("link", name=open_link)
        if self._click_if_visible(link, timeout=1_000):
            print("ClickOnce: clic en Abrir archivo del aviso de descargas.", flush=True)
            return

        raise AssertionError(f"El navegador no mostró un control de apertura para {MANIFEST} en el aviso.")

    def _app(self):
        browser = self.page.context.browser
        protocol = browser.new_browser_cdp_session()
        try:
            processes = protocol.send("SystemInfo.getProcessInfo")["processInfo"]
            pid = next(process["id"] for process in processes if process["type"] == "browser")
        finally:
            protocol.detach()
        # Chromium añade el nombre del navegador o del perfil tras este separador.
        title_prefix = f"{self.page.title()} - "
        return self.dcontext.app(pid=pid, title_prefix=title_prefix)

    @staticmethod
    def _click_if_visible(locator, *, timeout: int) -> bool:
        try:
            locator.wait_for(timeout=timeout)
        except DesktopTimeoutError:
            return False
        locator.click(timeout=timeout)
        return True
