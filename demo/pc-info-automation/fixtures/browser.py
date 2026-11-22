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

from config import (
    BROWSER_CONNECT_TIMEOUT_MS,
    BROWSER_POLL_INTERVAL_S,
    BROWSER_SHUTDOWN_TIMEOUT_S,
    BROWSER_STARTUP_TIMEOUT_S,
    DEMO_HOST,
)

BROWSER_NAME = "Microsoft Edge"
BROWSER_EXECUTABLE = Path("Microsoft/Edge/Application/msedge.exe")
BROWSER_ENV_VARS = ("PROGRAMFILES(X86)", "PROGRAMFILES", "LOCALAPPDATA")
PROFILE_PREFIX = "pc-info-browser-"
PREFERENCES_DIR = "Default"
PREFERENCES_FILE = "Preferences"
DOWNLOADS_DIR = "downloads"
DEVTOOLS_PORT_FILE = "DevToolsActivePort"
BLANK_PAGE_URL = "about:blank"
CDP_BROWSER_CLOSE = "Browser.close"
CDP_SET_DOWNLOAD_BEHAVIOR = "Browser.setDownloadBehavior"
BROWSER_FLAGS = (
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
)

# Comenta el bloque activo antes de descomentar el otro.

# Navegador que muestra el aviso nativo de ClickOnce
DOWNLOAD_BEHAVIOR = None

# Navegador que descarga el manifiesto y puede perder el parámetro operation
# DOWNLOAD_BEHAVIOR = "default"


@contextmanager
def open_browser_context(playwright: Playwright):
    executable = _browser_executable()
    with TemporaryDirectory(prefix=PROFILE_PREFIX) as profile:
        directory = Path(profile)
        preferences = directory / PREFERENCES_DIR / PREFERENCES_FILE
        preferences.parent.mkdir()
        preferences.write_text(json.dumps({
            "download": {"default_directory": str(directory / DOWNLOADS_DIR)},
        }), encoding="utf-8")
        process = Popen(
            [
                str(executable),
                f"--user-data-dir={profile}",
                *BROWSER_FLAGS,
                BLANK_PAGE_URL,
            ],
            stdout=DEVNULL,
            stderr=DEVNULL,
        )
        try:
            browser = playwright.chromium.connect_over_cdp(
                _debugging_endpoint(process, directory),
                no_defaults=True,
                timeout=BROWSER_CONNECT_TIMEOUT_MS,
            )
            try:
                if DOWNLOAD_BEHAVIOR is not None:
                    session = browser.new_browser_cdp_session()
                    try:
                        session.send(CDP_SET_DOWNLOAD_BEHAVIOR, {"behavior": DOWNLOAD_BEHAVIOR})
                    finally:
                        session.detach()
                yield browser.contexts[0]
            finally:
                with suppress(Error):
                    browser.new_browser_cdp_session().send(CDP_BROWSER_CLOSE)
        finally:
            try:
                process.wait(timeout=BROWSER_SHUTDOWN_TIMEOUT_S)
            except TimeoutExpired:
                process.terminate()
                process.wait(timeout=BROWSER_SHUTDOWN_TIMEOUT_S)


def _browser_executable() -> Path:
    for variable in BROWSER_ENV_VARS:
        if root := environ.get(variable):
            executable = Path(root) / BROWSER_EXECUTABLE
            if executable.is_file():
                return executable
    raise SkipTest(f"Instala {BROWSER_NAME} para ejecutar esta prueba.")


def _debugging_endpoint(process: Popen, profile: Path) -> str:
    port_file = profile / DEVTOOLS_PORT_FILE
    deadline = monotonic() + BROWSER_STARTUP_TIMEOUT_S
    while monotonic() < deadline:
        with suppress(FileNotFoundError):
            port, separator, _ = port_file.read_text(encoding="utf-8").partition("\n")
            if separator and port.isdecimal():
                return f"http://{DEMO_HOST}:{port}"
        if (exit_code := process.poll()) is not None:
            raise RuntimeError(
                f"{BROWSER_NAME} se cerró antes de iniciar la prueba (código de salida {exit_code})."
            )
        sleep(BROWSER_POLL_INTERVAL_S)
    raise TimeoutError(f"{BROWSER_NAME} no inició la conexión con Playwright.")
