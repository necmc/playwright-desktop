"""Abre el navegador con un perfil de pruebas desechable."""

from contextlib import contextmanager, suppress
import json
from os import environ
from pathlib import Path
from subprocess import DEVNULL, Popen, TimeoutExpired
from tempfile import TemporaryDirectory
from time import monotonic, sleep
from unittest import SkipTest

from playwright.sync_api import Error, Playwright


BROWSER_NAME = "Microsoft Edge"
BROWSER_EXECUTABLE = Path("Microsoft/Edge/Application/msedge.exe")

# Comenta el bloque activo antes de descomentar el otro.

# Navegador que muestra el aviso nativo de ClickOnce
DOWNLOAD_BEHAVIOR = None

# Navegador que descarga el manifiesto y puede perder el parámetro operation
# DOWNLOAD_BEHAVIOR = "default"


@contextmanager
def open_browser_context(playwright: Playwright):
    executable = _browser_executable()
    with TemporaryDirectory(prefix="pc-info-browser-") as profile:
        directory = Path(profile)
        preferences = directory / "Default" / "Preferences"
        preferences.parent.mkdir()
        preferences.write_text(json.dumps({
            "download": {"default_directory": str(directory / "downloads")},
        }), encoding="utf-8")
        process = Popen(
            [
                # Inicia el ejecutable de Edge.
                str(executable),
                # Usa el perfil temporal de la prueba.
                f"--user-data-dir={profile}",
                # Deja que Edge elija un puerto libre para Playwright.
                "--remote-debugging-port=0",
                # Omite la pantalla de bienvenida.
                "--no-first-run",
                # Omite la comprobación del navegador predeterminado.
                "--no-default-browser-check",
                # Evita que Edge se relance antes de aceptar conexiones por Chrome DevTools Protocol.
                "--edge-skip-compat-layer-relaunch",
                # Evita sincronizar el perfil temporal con la cuenta Microsoft de Windows.
                "--disable-sync",
                # Abre una pestaña en blanco.
                "about:blank",
            ],
            # Descarta la salida estándar de Edge.
            stdout=DEVNULL,
            # Descarta la salida de error de Edge.
            stderr=DEVNULL,
        )
        try:
            browser = playwright.chromium.connect_over_cdp(
                _debugging_endpoint(process, directory), no_defaults=True, timeout=10_000,
            )
            try:
                if DOWNLOAD_BEHAVIOR is not None:
                    session = browser.new_browser_cdp_session()
                    try:
                        session.send("Browser.setDownloadBehavior", {"behavior": DOWNLOAD_BEHAVIOR})
                    finally:
                        session.detach()
                yield browser.contexts[0]
            finally:
                with suppress(Error):
                    browser.new_browser_cdp_session().send("Browser.close")
        finally:
            try:
                process.wait(timeout=5)
            except TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)


def _browser_executable() -> Path:
    for variable in ("PROGRAMFILES(X86)", "PROGRAMFILES", "LOCALAPPDATA"):
        if root := environ.get(variable):
            executable = Path(root) / BROWSER_EXECUTABLE
            if executable.is_file():
                return executable
    raise SkipTest(f"Instala {BROWSER_NAME} para ejecutar esta prueba.")


def _debugging_endpoint(process: Popen, profile: Path) -> str:
    port_file = profile / "DevToolsActivePort"
    deadline = monotonic() + 10
    while monotonic() < deadline:
        with suppress(FileNotFoundError):
            port, separator, _ = port_file.read_text(encoding="utf-8").partition("\n")
            if separator and port.isdecimal():
                return f"http://127.0.0.1:{port}"
        if (exit_code := process.poll()) is not None:
            raise RuntimeError(
                f"{BROWSER_NAME} se cerró antes de iniciar la prueba (código de salida {exit_code})."
            )
        sleep(0.1)
    raise TimeoutError(f"{BROWSER_NAME} no inició la conexión con Playwright.")
