"""Compatibility checks for loading the native extension."""

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
