"""Configuración específica de Edge para la activación normal de ClickOnce."""

from contextlib import contextmanager
from tempfile import TemporaryDirectory

from playwright.sync_api import Playwright

@contextmanager
def open_clickonce_context(playwright: Playwright):
    with TemporaryDirectory(prefix="pc-info-edge-") as profile:
        context = playwright.chromium.launch_persistent_context(
            profile,
            channel="msedge",
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
            # Conservar el manejo de descargas de Edge sin aceptar permisos por el usuario.
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
