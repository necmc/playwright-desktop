"""Native desktop interactions for Playwright Python."""

__version__ = "0.1.0"

from ._errors import (
    AmbiguousMatchError, BridgeUnavailableError, DesktopActionError, DesktopBusyError,
    DesktopClosedError, DesktopConfigurationError, DesktopError, DesktopTimeoutError,
    PatchConflictError, StaleWindowError, UnsupportedPlatformError,
)
from ._patch import install, uninstall
from ._runtime import configure
from ._sync import Alert, App, DesktopContext, Window

install()

__all__ = [
    "Alert", "App", "DesktopContext", "Window",
    "configure", "install", "uninstall", "DesktopError", "DesktopTimeoutError", "PatchConflictError",
    "DesktopConfigurationError", "BridgeUnavailableError", "AmbiguousMatchError",
    "StaleWindowError", "DesktopActionError", "DesktopBusyError", "DesktopClosedError",
    "UnsupportedPlatformError",
]
