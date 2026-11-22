"""Compatibility checks and OS locale utilities for Playwright Desktop."""

import locale
import sys
import sysconfig

from ._errors import DesktopError, UnsupportedPlatformError

SUPPORTED_SYS_PLATFORM = "win32"
SUPPORTED_SYSCONFIG_PLATFORM = "win-amd64"
SUPPORTED_IMPLEMENTATION = "cpython"
FREE_THREADED_CONFIG_VAR = "Py_GIL_DISABLED"
DEFAULT_OS_LANGUAGE = "en"


def require_supported_platform() -> None:
    if (
        sys.platform != SUPPORTED_SYS_PLATFORM
        or sysconfig.get_platform() != SUPPORTED_SYSCONFIG_PLATFORM
        or sys.implementation.name != SUPPORTED_IMPLEMENTATION
        or sysconfig.get_config_var(FREE_THREADED_CONFIG_VAR)
    ):
        raise UnsupportedPlatformError("The native extension requires conventional CPython on Windows x64.")


def os_language() -> str:
    """Return the active OS UI display language tag in BCP-47 form (e.g. 'es-ES', 'en-US')."""
    from . import _backend

    try:
        tag = _backend.os_language()
        if tag:
            return tag
    except (UnsupportedPlatformError, DesktopError):
        pass
    lang, _ = locale.getlocale()
    return lang.replace("_", "-") if lang else DEFAULT_OS_LANGUAGE
