"""Validation and shared native invocation models for sync and async facades."""

from dataclasses import dataclass, field
from typing import Any, Callable

MAX_TIMEOUT_MS = 2**32 - 1
MAX_PROCESS_ID = 2**32 - 1

ROLE_TEXT = "text"
ROLE_TEXTBOX = "textbox"
ROLE_BUTTON = "button"
ROLE_LINK = "link"
ROLE_MENUITEM = "menuitem"
ROLE_DIALOG = "dialog"
ROLE_GROUP = "group"
ROLE_PANE = "pane"
ROLE_MENU = "menu"
ROLE_LISTITEM = "listitem"

CLICK_ROLES = (ROLE_BUTTON, ROLE_LINK, ROLE_MENUITEM)
SCOPE_ROLES = (ROLE_DIALOG, ROLE_GROUP, ROLE_PANE, ROLE_MENU, ROLE_LISTITEM)
SUPPORTED_ROLES = (ROLE_TEXT, ROLE_TEXTBOX, *CLICK_ROLES, *SCOPE_ROLES)

OP_PREPARE = "prepare"
OP_CAPTURE = "capture"
OP_WAIT_VISIBLE = "wait_visible"
OP_READ_TEXT = "read_text"
OP_FILL = "fill"
OP_CLICK = "click"
OP_WAIT_CLOSED = "wait_closed"
OP_BRING_TO_FRONT = "bring_to_front"
OP_CLOSE_WINDOW = "close_window"


@dataclass(frozen=True)
class InvokeParams:
    operation: str = ""
    window_id: int = 0
    capture_id: int = 0
    title: str = ""
    pid: int | None = None
    role: str = ""
    name: str = ""
    text: str = ""
    scope_role: str = ""
    scope_name: str = ""
    title_prefix: str = ""
    has_text: str = ""
    only_new: bool = False


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
    title: str | None = None
    pid: int | None = None
    title_prefix: str | None = None
    has_text: str | None = None
    only_new: bool = False
    allow_empty: bool = field(default=False, repr=False, compare=False)

    def __post_init__(self):
        if not isinstance(self.only_new, bool):
            raise ValueError("only_new must be a boolean.")
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
        if self.has_text is not None:
            validate_string(self.has_text, "has_text")
        if not self.allow_empty and (
            self.title is None and self.title_prefix is None and self.pid is None and self.has_text is None
        ):
            raise ValueError("A window title, title prefix, PID or visible text is required.")

    @classmethod
    def capture(
        cls,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
    ) -> "WindowQuery":
        return cls(
            title=title, pid=pid, title_prefix=title_prefix, has_text=has_text, allow_empty=True,
        )


def query_parameters(query: WindowQuery) -> dict[str, Any]:
    return {
        "title": query.title or "",
        "pid": query.pid,
        "title_prefix": query.title_prefix or "",
        "has_text": query.has_text or "",
        "only_new": query.only_new,
    }


def validate_predicate(predicate: Callable[[Any], bool] | None) -> Callable[[Any], bool] | None:
    if predicate is not None and not callable(predicate):
        raise ValueError("predicate must be callable.")
    return predicate


def validate_control(role: str, name: str) -> None:
    if role not in SUPPORTED_ROLES:
        raise ValueError("Unsupported desktop role.")
    validate_string(name, "name")


def validate_scope(role: str, name: str, scope: tuple[str, str] | None) -> tuple[str, str]:
    if scope is not None or role not in SCOPE_ROLES:
        raise ValueError("Scoped lookup requires a window-level dialog, group, pane, menu or list item.")
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
