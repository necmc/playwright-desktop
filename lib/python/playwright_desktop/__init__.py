"""Native desktop interactions for Playwright Python."""

from importlib.metadata import PackageNotFoundError as _PackageNotFoundError, version as _package_version

_PACKAGE_NAME = "playwright-desktop"
_FALLBACK_VERSION = "0.0.0"

try:
    __version__ = _package_version(_PACKAGE_NAME)
except _PackageNotFoundError:
    __version__ = _FALLBACK_VERSION

from ._errors import (
    AmbiguousMatchError, BridgeUnavailableError, DesktopActionError, DesktopBusyError,
    DesktopClosedError, DesktopConfigurationError, DesktopError, DesktopTimeoutError,
    PatchConflictError, StaleWindowError, UnsupportedPlatformError,
)
from ._patch import install, uninstall
from ._platform import os_language
from ._runtime import configure
from ._sync import Alert, App, DesktopContext, Window

install()

__all__ = [
    "Alert", "App", "DesktopContext", "Window",
    "configure", "install", "os_language", "uninstall", "DesktopError", "DesktopTimeoutError",
    "PatchConflictError", "DesktopConfigurationError", "BridgeUnavailableError", "AmbiguousMatchError",
    "StaleWindowError", "DesktopActionError", "DesktopBusyError", "DesktopClosedError",
    "UnsupportedPlatformError",
]
