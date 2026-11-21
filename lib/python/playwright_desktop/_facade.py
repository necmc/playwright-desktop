"""Shared non-IO facade state and encapsulation helpers for sync and async APIs."""

from __future__ import annotations

from typing import Any, Callable
import weakref

from ._errors import DesktopError
from ._models import WindowQuery
from ._platform import os_language


class _DesktopContextBase:
    """Shared context state correlated with a desktop Session."""

    def __init__(self, session):
        self._session = session
        self._apps: dict[int, weakref.ref] = {}

    @property
    def browser_context(self):
        return self._session.browser_context

    @property
    def os_language(self) -> str:
        return os_language()

    @property
    def apps(self) -> tuple[Any, ...]:
        if self._session.is_closed():
            self._apps.clear()
            return ()
        live = []
        stale = []
        for handle, ref in self._apps.items():
            item = ref()
            if item is None:
                stale.append(handle)
            else:
                live.append(item)
        for handle in stale:
            self._apps.pop(handle, None)
        return tuple(live)

    def set_default_timeout(self, timeout: float) -> None:
        self._session.set_default_timeout(timeout)

    def _register_app(self, app: Any) -> None:
        if app._handle:
            self._apps[app._handle] = weakref.ref(app)

    def _unregister_app(self, handle: int) -> None:
        self._apps.pop(handle, None)

    def _clear_apps(self) -> None:
        self._apps.clear()


class _WindowBase:
    """Shared encapsulated metadata and handle tracking for App, Window and Alert."""

    def __init__(
        self,
        session,
        dcontext: Any,
        query: WindowQuery,
        handle: int = 0,
        *,
        pid: int | None = None,
        title: str | None = None,
    ):
        self._session = session
        self._dcontext = dcontext
        self._query = query
        self._handle = handle
        self._pid = pid if pid is not None else query.pid
        self._title = title if title is not None else query.title

    @property
    def context(self):
        return self._dcontext

    @property
    def pid(self) -> int | None:
        return self._pid

    @property
    def title(self) -> str | None:
        return self._title

    def _apply_reply(self, reply: Any) -> None:
        handle = getattr(reply, "handle", 0)
        if handle:
            self._handle = handle
        pid = getattr(reply, "pid", None)
        if pid is not None:
            self._pid = pid
        title = getattr(reply, "title", None)
        if title is not None:
            self._title = title


def _accept_captured(
    candidate: Any,
    predicate: Callable[[Any], bool] | None,
    *,
    register: Callable[[Any], None] | None = None,
) -> Any:
    if predicate is not None and not predicate(candidate):
        raise DesktopError("Captured desktop window did not satisfy the expected predicate.")
    if register is not None:
        register(candidate)
    return candidate
