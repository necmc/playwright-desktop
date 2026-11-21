"""Confirmación opcional del instalador ClickOnce de pc-info."""

from urllib.parse import urlsplit

from playwright_desktop import DesktopContext, DesktopTimeoutError

_LABELS = {
    True: ("Ejecución de la aplicación - Advertencia de seguridad", "Ejecutar"),
    False: ("Application Run - Security Warning", "Run"),
}


class ClickOnceInstaller:
    def __init__(self, dcontext: DesktopContext, *, application_name: str, base_url: str):
        self.application_name = application_name
        self.source = urlsplit(base_url).hostname
        title, self._run_button = _LABELS[dcontext.os_language.lower().startswith("es")]
        self._notice = dcontext.app(title=title, only_new=True)

    def confirm_if_present(self) -> bool:
        try:
            self._notice.get_by_role("button", name=self._run_button).wait_for(timeout=0)
        except DesktopTimeoutError:
            return False

        for label in (self.application_name, self.source):
            try:
                self._notice.get_by_role("text", name=label).wait_for(timeout=1_000)
            except DesktopTimeoutError:
                raise AssertionError(f"El aviso de ClickOnce no muestra «{label}».") from None

        self._notice.get_by_role("button", name=self._run_button).click(timeout=3_000)
        print(f"ClickOnce: clic en {self._run_button} para {self.application_name}.", flush=True)
        return True
