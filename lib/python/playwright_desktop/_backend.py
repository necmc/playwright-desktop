"""Load the desktop extension and resolve Java Access Bridge settings."""

from importlib import import_module
import os
from pathlib import Path

from ._errors import DesktopError
from ._platform import require_supported_platform

MAX_JAB_TEXT_UNITS = 1023


def load_extension():
    require_supported_platform()
    try:
        return import_module("playwright_desktop._native")
    except ImportError as error:
        raise DesktopError("The native extension is not installed. Build and install the playwright-desktop wheel.") from error


def normalize_java_home(java_home: str | os.PathLike[str]) -> str:
    path = os.fspath(java_home)
    if not isinstance(path, str) or not path or "\0" in path:
        raise ValueError("java_home must be a nonempty filesystem path.")
    return str(Path(path).expanduser().absolute())


def session_argument(java_home: str | None) -> str | None:
    if java_home is not None:
        return java_home
    if configured := os.environ.get("JAVA_HOME"):
        return normalize_java_home(configured)
    return None


def validate_text_capacity(utf16_units: int) -> None:
    if utf16_units > MAX_JAB_TEXT_UNITS:
        raise ValueError(f"Java Access Bridge accepts at most {MAX_JAB_TEXT_UNITS} UTF-16 code units.")
