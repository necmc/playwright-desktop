"""Validation shared by synchronous and asynchronous facades."""

from dataclasses import dataclass
import math


def validate_string(value: str, label: str) -> str:
    if not isinstance(value, str) or not value or "\0" in value:
        raise ValueError(f"{label} must be a nonempty string without NUL characters.")
    try:
        value.encode("utf-16-le")
    except UnicodeEncodeError as error:
        raise ValueError(f"{label} contains invalid Unicode.") from error
    return value


def validate_timeout(value: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0 or value > 2**32 - 1:
        raise ValueError("timeout must be a finite number between 0 and 4294967295 milliseconds.")
    return value


@dataclass(frozen=True)
class WindowQuery:
    title: str
    pid: int | None = None

    def __post_init__(self):
        validate_string(self.title, "title")
        if self.pid is not None and (isinstance(self.pid, bool) or not isinstance(self.pid, int) or not 0 < self.pid <= 2**32 - 1):
            raise ValueError("pid must be a positive Windows process identifier.")


def validate_control(role: str, name: str) -> None:
    if role not in ("textbox", "button"):
        raise ValueError("Supported desktop roles are 'textbox' and 'button'.")
    validate_string(name, "name")


def validate_text(text: str) -> None:
    if not isinstance(text, str) or "\0" in text:
        raise ValueError("text must be a string without NUL characters.")
    try:
        size = len(text.encode("utf-16-le")) // 2
    except UnicodeEncodeError as error:
        raise ValueError("text contains invalid Unicode.") from error
    if size > 1023:
        raise ValueError("Java Access Bridge accepts at most 1023 UTF-16 code units.")
