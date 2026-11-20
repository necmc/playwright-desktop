"""Aviso de MS SScreen."""

from playwright.sync_api import Page
from playwright_desktop import DesktopTimeoutError


class SScreenWarning:
    def __init__(self, page: Page):
        self.page = page
        es_window = page.desktop.window(has_text="Windows protegió su PC")
        en_window = page.desktop.window(has_text="Windows protected your PC")
        self.es_details = es_window.get_by_role("link", name="Más información")
        self.es_run_anyway = es_window.get_by_role("button", name="Ejecutar de todas formas")
        self.en_details = en_window.get_by_role("link", name="More info")
        self.en_run_anyway = en_window.get_by_role("button", name="Run anyway")

    def confirm_if_present(self) -> bool:
        for details, run_anyway in (
            (self.es_details, self.es_run_anyway),
            (self.en_details, self.en_run_anyway),
        ):
            try:
                details.wait_for(timeout=0)
            except DesktopTimeoutError:
                continue

            details.click(timeout=3_000)
            run_anyway.wait_for(timeout=3_000)
            self.page.wait_for_timeout(700)
            run_anyway.click(timeout=3_000)
            return True

        return False
