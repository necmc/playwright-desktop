"""Configuration and loading of the Windows Java Access Bridge backend."""

from importlib import import_module
import os
from pathlib import Path
import sys
import sysconfig

from ._errors import DesktopConfigurationError, DesktopError, UnsupportedPlatformError

MAX_JAB_TEXT_UNITS = 1023


def load_extension():
    if (
        sys.platform != "win32"
        or sysconfig.get_platform() != "win-amd64"
        or sysconfig.get_config_var("Py_GIL_DISABLED")
    ):
        raise UnsupportedPlatformError("The native backend requires conventional CPython on Windows x64.")
    try:
        return import_module("playwright_desktop._native")
    except ImportError as error:
        raise DesktopError("The native extension is not installed. Build and install the playwright-desktop wheel.") from error


def normalize_java_home(java_home: str | os.PathLike[str]) -> str:
    path = os.fspath(java_home)
    if not isinstance(path, str) or not path or "\0" in path:
        raise ValueError("java_home must be a nonempty filesystem path.")
    return str(Path(path).expanduser().absolute())


def session_argument(java_home: str | None) -> str:
    if java_home is not None:
        return java_home
    if configured := os.environ.get("JAVA_HOME"):
        return normalize_java_home(configured)
    raise DesktopConfigurationError("Set JAVA_HOME or call page.desktop.configure(java_home=...).")


def validate_text_capacity(utf16_units: int) -> None:
    if utf16_units > MAX_JAB_TEXT_UNITS:
        raise ValueError(f"Java Access Bridge accepts at most {MAX_JAB_TEXT_UNITS} UTF-16 code units.")
