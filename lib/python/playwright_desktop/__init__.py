"""Native desktop interactions for Playwright Python."""

__version__ = "0.1.0"

from ._errors import (
    AmbiguousMatchError, BridgeUnavailableError, DesktopActionError, DesktopBusyError,
    DesktopClosedError, DesktopConfigurationError, DesktopError, DesktopTimeoutError,
    PatchConflictError, StaleWindowError, UnsupportedPlatformError,
)
from ._patch import install, uninstall

install()

__all__ = [
    "install", "uninstall", "DesktopError", "DesktopTimeoutError", "PatchConflictError",
    "DesktopConfigurationError", "BridgeUnavailableError", "AmbiguousMatchError",
    "StaleWindowError", "DesktopActionError", "DesktopBusyError", "DesktopClosedError",
    "UnsupportedPlatformError",
]
