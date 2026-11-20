"""Selecciona el navegador de Windows y lo abre con un perfil de pruebas."""

from contextlib import contextmanager, nullcontext
import ctypes
from ctypes import wintypes
import json
from os import environ
from pathlib import Path
import sys
from tempfile import TemporaryDirectory

from playwright.sync_api import Playwright


BROWSER_NAMES = {
    "msedge.exe": "Microsoft Edge",
    "chrome.exe": "Google Chrome",
    "chromium.exe": "Chromium",
    "brave.exe": "Brave",
    "vivaldi.exe": "Vivaldi",
    "opera.exe": "Opera",
}
FALLBACK_PATHS = (
    "Microsoft/Edge/Application/msedge.exe",
    "Google/Chrome/Application/chrome.exe",
)


def default_browser_executable() -> Path | None:
    if sys.platform != "win32":
        return None

    assocf_is_protocol = 0x1000
    assocstr_executable = 2
    query = ctypes.windll.shlwapi.AssocQueryStringW
    query.argtypes = (
        wintypes.DWORD, wintypes.DWORD, wintypes.LPCWSTR, wintypes.LPCWSTR,
        wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD),
    )
    query.restype = ctypes.c_long
    length = wintypes.DWORD(32768)
    path = ctypes.create_unicode_buffer(length.value)
    if query(assocf_is_protocol, assocstr_executable, "http", None, path, ctypes.byref(length)) == 0:
        return Path(path.value)
    return None


def select_browser() -> Path:
    preferred = default_browser_executable()
    if preferred and preferred.name.lower() in BROWSER_NAMES and preferred.is_file():
        return preferred

    for relative_path in FALLBACK_PATHS:
        for folder in ("ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"):
            if root := environ.get(folder):
                candidate = Path(root) / relative_path
                if candidate.is_file():
                    name = BROWSER_NAMES[candidate.name.lower()]
                    print(f"No se pudo usar el navegador predeterminado para la prueba. Se abrirá {name}.", flush=True)
                    return candidate
    raise RuntimeError("Para ejecutar la prueba, usa un navegador Chromium compatible con Playwright, como Edge o Chrome.")


@contextmanager
def open_clickonce_context(playwright: Playwright, executable: Path, *, use_extension_profile: bool):
    profile = None
    if use_extension_profile and executable.name.lower() != "msedge.exe":
        name = BROWSER_NAMES[executable.name.lower()]
        extension_profile = Path(environ["LOCALAPPDATA"]) / "playwright-desktop" / "pc-info" / executable.stem.lower()
        print(f"Para abrir ClickOnce en {name}, usa una extensión compatible en el perfil de prueba o usa Edge.", flush=True)
        print(f"Perfil de prueba para extensiones: {extension_profile}", flush=True)
        if extension_profile.is_dir():
            profile = extension_profile

    profile_context = nullcontext(profile) if profile else TemporaryDirectory(prefix="pc-info-browser-")
    with profile_context as user_data_dir:
        if profile is None:
            directory = Path(user_data_dir)
            preferences = directory / "Default" / "Preferences"
            preferences.parent.mkdir()
            preferences.write_text(json.dumps({
                "download": {"default_directory": str(directory / "downloads")},
            }), encoding="utf-8")
        context = playwright.chromium.launch_persistent_context(
            user_data_dir,
            executable_path=executable,
            headless=False,
            accept_downloads=False,
            ignore_default_args=[
                "--disable-extensions",
                "--disable-component-extensions-with-background-pages",
                "--disable-default-apps",
                "--enable-automation",
            ],
        )
        try:
            # Conservar el manejo del navegador para las descargas que activan ClickOnce.
            protocol = context.browser.new_browser_cdp_session()
            context_ids = protocol.send("Target.getBrowserContexts")["browserContextIds"]
            options = {"behavior": "default"}
            if context_ids:
                options["browserContextId"] = context_ids[0]
            protocol.send("Browser.setDownloadBehavior", options)
            protocol.detach()
            yield context
        finally:
            context.close()
