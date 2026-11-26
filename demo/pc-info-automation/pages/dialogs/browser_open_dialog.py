"""Page Object del aviso de apertura y descargas en el navegador."""

from playwright.sync_api import Page
from playwright_desktop import DesktopContext, DesktopTimeoutError

SPANISH_LANG_CHECK = "navigator.language.startsWith('es')"
CDP_PROCESS_INFO_COMMAND = "SystemInfo.getProcessInfo"
BROWSER_PROCESS_TYPE = "browser"
TITLE_SEPARATOR = " - "
OPEN_NOTICE_TIMEOUT_MS = 8000
DOWNLOAD_LINK_TIMEOUT_MS = 1000


class BrowserOpenDialog:
    def __init__(self, dcontext: DesktopContext, page: Page):
        self.dcontext = dcontext
        self.page = page

    def click_open(self) -> None:
        browser_window = self._browser_window()
        if self.page.evaluate(SPANISH_LANG_CHECK):
            confirmation_title, open_button = "¿Quieres abrir este archivo?", "Abrir"
            open_link = "Abrir archivo"
        else:
            confirmation_title, open_button = "Do you want to open this file?", "Open"
            open_link = "Open file"

        notice = browser_window.get_by_role("pane", name=confirmation_title)
        if self._click_if_visible(notice.get_by_role("button", name=open_button), timeout=OPEN_NOTICE_TIMEOUT_MS):
            print("ClickOnce: clic en Abrir del aviso del navegador.", flush=True)
            return

        item = browser_window.get_by_role("listitem", name="InformationDemo.application")
        link = item.get_by_role("link", name=open_link)
        if self._click_if_visible(link, timeout=DOWNLOAD_LINK_TIMEOUT_MS):
            print("ClickOnce: clic en Abrir archivo del aviso de descargas.", flush=True)
            return

        raise AssertionError("El navegador no mostró un control de apertura para InformationDemo.application en el aviso.")

    def _browser_window(self):
        browser = self.page.context.browser
        protocol = browser.new_browser_cdp_session()
        try:
            processes = protocol.send(CDP_PROCESS_INFO_COMMAND)["processInfo"]
            pid = next(process["id"] for process in processes if process["type"] == BROWSER_PROCESS_TYPE)
        finally:
            protocol.detach()
        # Chromium añade el nombre del navegador o del perfil tras este separador.
        title_prefix = f"{self.page.title()}{TITLE_SEPARATOR}"
        return self.dcontext.app(pid=pid, title_prefix=title_prefix)

    @staticmethod
    def _click_if_visible(locator, *, timeout: int) -> bool:
        try:
            locator.wait_for(timeout=timeout)
        except DesktopTimeoutError:
            return False
        locator.click(timeout=timeout)
        return True
