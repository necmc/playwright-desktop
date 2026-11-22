"""Public desktop errors, compatible with Playwright's error hierarchy."""

from playwright.sync_api import Error, TimeoutError as PlaywrightTimeoutError

DEFAULT_NATIVE_ERROR_CODE = "ACTION"


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


def translate_native_error(error: Exception) -> Exception:
    code, message = error.args if len(error.args) == 2 else (DEFAULT_NATIVE_ERROR_CODE, str(error))
    return NATIVE_ERRORS.get(code, DesktopError)(message)
