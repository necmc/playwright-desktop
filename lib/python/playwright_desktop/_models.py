"""Validation shared by synchronous and asynchronous facades."""

from dataclasses import dataclass

MAX_TIMEOUT_MS = 2**32 - 1
MAX_PROCESS_ID = 2**32 - 1
CLICK_ROLES = ("button", "link", "menuitem")
SCOPE_ROLES = ("dialog", "group", "menu", "listitem")


def validate_string(value: str, label: str) -> str:
    if not isinstance(value, str) or not value or "\0" in value:
        raise ValueError(f"{label} must be a nonempty string without NUL characters.")
    _validate_unicode(value, label)
    return value


def validate_timeout(value: float) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not 0 <= value <= MAX_TIMEOUT_MS:
        raise ValueError(f"timeout must be a finite number between 0 and {MAX_TIMEOUT_MS} milliseconds.")
    return value


@dataclass(frozen=True)
class WindowQuery:
    title: str | None
    pid: int | None = None
    title_prefix: str | None = None

    def __post_init__(self):
        if self.pid is not None and (
            isinstance(self.pid, bool) or not isinstance(self.pid, int) or not 0 < self.pid <= MAX_PROCESS_ID
        ):
            raise ValueError("pid must be a positive process identifier.")
        if self.title is not None and self.title_prefix is not None:
            raise ValueError("Use either title or title_prefix.")
        if self.title is not None:
            validate_string(self.title, "title")
        elif self.title_prefix is not None:
            validate_string(self.title_prefix, "title_prefix")
        elif self.pid is None:
            raise ValueError("A window title, title prefix or PID is required.")


def validate_control(role: str, name: str) -> None:
    if role not in ("textbox", *CLICK_ROLES, *SCOPE_ROLES):
        raise ValueError("Unsupported desktop role.")
    validate_string(name, "name")


def validate_scope(role: str, name: str, scope: tuple[str, str] | None) -> tuple[str, str]:
    if scope is not None or role not in SCOPE_ROLES:
        raise ValueError("Scoped lookup requires a window-level dialog, group, menu or list item.")
    return role, name


def validate_click_role(role: str) -> None:
    if role not in CLICK_ROLES:
        raise ValueError("click() requires a button, link or menu item.")


def validate_action_role(role: str, required_role: str, action: str) -> None:
    if role != required_role:
        raise ValueError(f"{action}() requires the {required_role} role.")


def validate_text(text: str) -> None:
    if not isinstance(text, str) or "\0" in text:
        raise ValueError("text must be a string without NUL characters.")
    _validate_unicode(text, "text")


def _validate_unicode(value: str, label: str) -> None:
    try:
        value.encode("utf-8")
    except UnicodeEncodeError as error:
        raise ValueError(f"{label} contains invalid Unicode.") from error
