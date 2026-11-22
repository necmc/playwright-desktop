"""Aviso de MS SScreen."""

from playwright.sync_api import Page
from playwright_desktop import DesktopContext, DesktopTimeoutError

SPANISH_LANG_PREFIX = "es"
DETECTION_TIMEOUT_MS = 0
ACTION_TIMEOUT_MS = 3000
POST_REVEAL_DELAY_MS = 200


class SScreenWarning:
    def __init__(self, dcontext: DesktopContext, page: Page):
        self.page = page
        if dcontext.os_language.lower().startswith(SPANISH_LANG_PREFIX):
            heading, self._details_name, self._run_name = (
                "Windows protegió su PC", "Más información", "Ejecutar de todas formas"
            )
        else:
            heading, self._details_name, self._run_name = (
                "Windows protected your PC", "More info", "Run anyway"
            )
        self._app = dcontext.app(has_text=heading, only_new=True)
        self._details = self._app.get_by_role("link", name=self._details_name)

    def confirm_if_present(self) -> bool:
        try:
            self._details.wait_for(timeout=DETECTION_TIMEOUT_MS)
        except DesktopTimeoutError:
            return False

        self._details.click(timeout=ACTION_TIMEOUT_MS)
        run_anyway = self._app.get_by_role("button", name=self._run_name)
        run_anyway.wait_for(timeout=ACTION_TIMEOUT_MS)
        self.page.wait_for_timeout(POST_REVEAL_DELAY_MS)
        run_anyway.click(timeout=ACTION_TIMEOUT_MS)
        print(f"SScreen: clic en {self._details_name} y {self._run_name}.", flush=True)
        return True
