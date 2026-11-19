"""Validation shared by synchronous and asynchronous facades."""

from dataclasses import dataclass

from ._backend import validate_text_capacity

MAX_TIMEOUT_MS = 2**32 - 1
MAX_PROCESS_ID = 2**32 - 1


def validate_string(value: str, label: str) -> str:
    if not isinstance(value, str) or not value or "\0" in value:
        raise ValueError(f"{label} must be a nonempty string without NUL characters.")
    _utf16(value, label)
    return value


def validate_timeout(value: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not 0 <= value <= MAX_TIMEOUT_MS:
        raise ValueError(f"timeout must be a finite number between 0 and {MAX_TIMEOUT_MS} milliseconds.")
    return value


@dataclass(frozen=True)
class WindowQuery:
    title: str
    pid: int | None = None

    def __post_init__(self):
        validate_string(self.title, "title")
        if self.pid is not None and (
            isinstance(self.pid, bool) or not isinstance(self.pid, int) or not 0 < self.pid <= MAX_PROCESS_ID
        ):
            raise ValueError("pid must be a positive process identifier.")


def validate_control(role: str, name: str) -> None:
    if role not in ("textbox", "button"):
        raise ValueError("Supported desktop roles are 'textbox' and 'button'.")
    validate_string(name, "name")


def validate_action_role(role: str, required_role: str, action: str) -> None:
    if role != required_role:
        raise ValueError(f"{action}() requires the {required_role} role.")


def validate_text(text: str) -> None:
    if not isinstance(text, str) or "\0" in text:
        raise ValueError("text must be a string without NUL characters.")
    validate_text_capacity(len(_utf16(text, "text")) // 2)


def _utf16(value: str, label: str) -> bytes:
    try:
        return value.encode("utf-16-le")
    except UnicodeEncodeError as error:
        raise ValueError(f"{label} contains invalid Unicode.") from error
