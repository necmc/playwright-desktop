"""Public desktop errors, compatible with Playwright's error hierarchy."""

from playwright.sync_api import Error, TimeoutError as PlaywrightTimeoutError


class DesktopError(Error):
    pass


class DesktopTimeoutError(DesktopError, PlaywrightTimeoutError):
    pass


class PatchConflictError(DesktopError):
    pass


class DesktopConfigurationError(DesktopError):
    pass


class BridgeUnavailableError(DesktopError):
    pass


class AmbiguousMatchError(DesktopError):
    pass


class StaleWindowError(DesktopError):
    pass


class DesktopActionError(DesktopError):
    pass


class DesktopBusyError(DesktopError):
    pass


class DesktopClosedError(DesktopError):
    pass


class UnsupportedPlatformError(DesktopError):
    pass


NATIVE_ERRORS = {
    "INVALID_ARGUMENT": ValueError,
    "TIMEOUT": DesktopTimeoutError,
    "CONFIGURATION": DesktopConfigurationError,
    "BRIDGE": BridgeUnavailableError,
    "AMBIGUOUS": AmbiguousMatchError,
    "STALE": StaleWindowError,
    "ACTION": DesktopActionError,
    "BUSY": DesktopBusyError,
    "CLOSED": DesktopClosedError,
    "CANCELLED": DesktopClosedError,
    "UNSUPPORTED": UnsupportedPlatformError,
}
