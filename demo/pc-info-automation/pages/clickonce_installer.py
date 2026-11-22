"""Confirmación opcional del instalador ClickOnce de pc-info."""

from urllib.parse import urlsplit

from playwright_desktop import DesktopContext, DesktopTimeoutError

SPANISH_LANG_PREFIX = "es"
DETECTION_TIMEOUT_MS = 0
LABEL_TIMEOUT_MS = 1000
CLICK_TIMEOUT_MS = 3000


class ClickOnceInstaller:
    def __init__(self, dcontext: DesktopContext, *, application_name: str, base_url: str):
        self.application_name = application_name
        self.source = urlsplit(base_url).hostname
        if dcontext.os_language.lower().startswith(SPANISH_LANG_PREFIX):
            title, self._run_button = "Ejecución de la aplicación - Advertencia de seguridad", "Ejecutar"
        else:
            title, self._run_button = "Application Run - Security Warning", "Run"
        self._notice = dcontext.app(title=title, only_new=True)

    def confirm_if_present(self) -> bool:
        try:
            self._notice.get_by_role("button", name=self._run_button).wait_for(timeout=DETECTION_TIMEOUT_MS)
        except DesktopTimeoutError:
            return False

        for label in (self.application_name, self.source):
            try:
                self._notice.get_by_role("text", name=label).wait_for(timeout=LABEL_TIMEOUT_MS)
            except DesktopTimeoutError:
                raise AssertionError(f"El aviso de ClickOnce no muestra «{label}».") from None

        self._notice.get_by_role("button", name=self._run_button).click(timeout=CLICK_TIMEOUT_MS)
        print(f"ClickOnce: clic en {self._run_button} para {self.application_name}.", flush=True)
        return True
