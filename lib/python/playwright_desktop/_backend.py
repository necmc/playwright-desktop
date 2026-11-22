"""Load the desktop extension and resolve Java Access Bridge settings."""

from importlib import import_module
import os
from pathlib import Path

from ._errors import DesktopError, translate_native_error
from ._platform import require_supported_platform

NATIVE_MODULE_NAME = "playwright_desktop._native"
JAVA_HOME_ENV_VAR = "JAVA_HOME"


def load_extension():
    require_supported_platform()
    try:
        return import_module(NATIVE_MODULE_NAME)
    except ImportError as error:
        raise DesktopError("The native extension is not installed. Build and install the playwright-desktop wheel.") from error


def os_language() -> str:
    native = load_extension()
    try:
        return native.os_language()
    except native.NativeError as error:
        raise translate_native_error(error) from error


def normalize_java_home(java_home: str | os.PathLike[str]) -> str:
    path = os.fspath(java_home)
    if not isinstance(path, str) or not path or "\0" in path:
        raise ValueError("java_home must be a nonempty filesystem path.")
    return str(Path(path).expanduser().absolute())


def session_argument(java_home: str | None) -> str | None:
    if java_home is not None:
        return java_home
    if configured := os.environ.get(JAVA_HOME_ENV_VAR):
        return normalize_java_home(configured)
    return None
