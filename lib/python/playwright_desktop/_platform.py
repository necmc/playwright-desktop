"""Compatibility checks and OS locale utilities for Playwright Desktop."""

import locale
import sys
import sysconfig

from ._errors import DesktopError, UnsupportedPlatformError


def require_supported_platform() -> None:
    if (
        sys.platform != "win32"
        or sysconfig.get_platform() != "win-amd64"
        or sys.implementation.name != "cpython"
        or sysconfig.get_config_var("Py_GIL_DISABLED")
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
    return lang.replace("_", "-") if lang else "en"
