"""Compatibility checks and OS locale utilities for Playwright Desktop."""

import ctypes
import locale
import sys
import sysconfig

from ._errors import UnsupportedPlatformError


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
    if sys.platform == "win32":
        kernel32 = ctypes.windll.kernel32
        lang_id = kernel32.GetUserDefaultUILanguage()
        buffer = ctypes.create_unicode_buffer(85)
        if kernel32.LCIDToLocaleName(lang_id, buffer, len(buffer), 0) > 0 and buffer.value:
            return buffer.value
    lang, _ = locale.getlocale()
    return lang.replace("_", "-") if lang else "en"
