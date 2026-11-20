"""Confirmación opcional del instalador ClickOnce de pc-info."""

from urllib.parse import urlsplit

from playwright.sync_api import Page
from playwright_desktop import DesktopTimeoutError


class ClickOnceInstaller:
    def __init__(self, page: Page, *, application_name: str, base_url: str):
        self.page = page
        self.application_name = application_name
        self.source = urlsplit(base_url).hostname
        self.es_notice = page.desktop.window(title="Ejecución de la aplicación - Advertencia de seguridad")
        self.en_notice = page.desktop.window(title="Application Run - Security Warning")

    def confirm_if_present(self) -> bool:
        for notice, run_button in ((self.es_notice, "Ejecutar"), (self.en_notice, "Run")):
            try:
                notice.get_by_role("button", name=run_button).wait_for(timeout=0)
            except DesktopTimeoutError:
                continue

            for label in (self.application_name, self.source):
                try:
                    notice.get_by_role("text", name=label).wait_for(timeout=1_000)
                except DesktopTimeoutError:
                    raise AssertionError(f"El aviso de ClickOnce no muestra «{label}».") from None

            notice.get_by_role("button", name=run_button).click(timeout=3_000)
            print(f"ClickOnce: clic en {run_button} para {self.application_name}.", flush=True)
            return True

        return False
