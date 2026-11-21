"""Aviso de MS SScreen."""

from playwright.sync_api import Page
from playwright_desktop import DesktopContext, DesktopTimeoutError

_LABELS = {
    True: ("Windows protegió su PC", "Más información", "Ejecutar de todas formas"),
    False: ("Windows protected your PC", "More info", "Run anyway"),
}


class SScreenWarning:
    def __init__(self, dcontext: DesktopContext, page: Page):
        self.page = page
        heading, self._details_name, self._run_name = _LABELS[
            dcontext.os_language.lower().startswith("es")
        ]
        self._app = dcontext.app(has_text=heading, only_new=True)
        self._details = self._app.get_by_role("link", name=self._details_name)

    def confirm_if_present(self) -> bool:
        try:
            self._details.wait_for(timeout=0)
        except DesktopTimeoutError:
            return False

        self._details.click(timeout=3_000)
        run_anyway = self._app.get_by_role("button", name=self._run_name)
        run_anyway.wait_for(timeout=3_000)
        self.page.wait_for_timeout(200)
        run_anyway.click(timeout=3_000)
        print(f"SScreen: clic en {self._details_name} y {self._run_name}.", flush=True)
        return True
