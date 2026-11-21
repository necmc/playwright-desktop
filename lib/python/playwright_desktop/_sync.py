"""Synchronous native desktop API aligned with Playwright BrowserContext/Page."""

from __future__ import annotations

from typing import Callable, Generic, TypeVar

from ._models import (
    WindowQuery,
    accept_captured,
    apply_reply_metadata,
    query_parameters,
    validate_action_role,
    validate_click_role,
    validate_control,
    validate_predicate,
    validate_scope,
    validate_text,
)
from ._timing import Deadline

_T = TypeVar("_T")


class DesktopContext:
    """Desktop environment correlated 1:1 with a Playwright BrowserContext."""

    def __init__(self, session):
        self._session = session

    @property
    def browser_context(self):
        return self._session.browser_context

    @property
    def apps(self) -> tuple[App, ...]:
        return self._session.tracked_apps()

    def set_default_timeout(self, timeout: float) -> None:
        self._session.set_default_timeout(timeout)

    def app(
        self,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
    ) -> App:
        return App(self._session, self, WindowQuery(title, pid, title_prefix, has_text))

    def expect_app(
        self,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[App], bool] | None = None,
        timeout: float | None = None,
    ) -> _Expectation[App]:
        query = WindowQuery.capture(title=title, pid=pid, title_prefix=title_prefix, has_text=has_text)
        validate_predicate(predicate)
        return _Expectation(
            self._session,
            self,
            query,
            timeout,
            lambda reply: App(
                self._session,
                self,
                query,
                handle=reply.handle,
                pid=getattr(reply, "pid", None),
                title=getattr(reply, "title", None),
            ),
            predicate=predicate,
            register=self._session.register_app,
        )

    def close(self) -> None:
        self._session.close_nowait()


class _WindowBase:
    def __init__(
        self,
        session,
        dcontext: DesktopContext,
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
    def context(self) -> DesktopContext:
        return self._dcontext

    @property
    def pid(self) -> int | None:
        return self._pid

    @property
    def title(self) -> str | None:
        return self._title

    def get_by_role(self, role: str, *, name: str) -> Locator:
        validate_control(role, name)
        return Locator(self, role, name)

    def _call(self, operation: str, timeout: float | None = None, **parameters):
        reply = self._session.call(
            operation,
            window_id=self._handle,
            **query_parameters(self._query),
            timeout=timeout,
            **parameters,
        )
        apply_reply_metadata(self, reply)
        return reply

    def bring_to_front(self, *, timeout: float | None = None) -> None:
        self._call("bring_to_front", timeout)

    def wait_for_closed(self, *, timeout: float | None = None) -> None:
        self._call("wait_closed", timeout)

    def _on_closed(self) -> None:
        pass

    def close(self, *, timeout: float | None = None) -> None:
        self._call("close_window", timeout)
        self._on_closed()


class Window(_WindowBase):
    """Secondary or modal window belonging to an App."""

    def __init__(self, session, dcontext: DesktopContext, app: App, query: WindowQuery, handle: int = 0, *, pid=None, title=None):
        super().__init__(session, dcontext, query, handle, pid=pid, title=title)
        self._app = app

    @property
    def app(self) -> App:
        return self._app


class Alert(Window):
    """Alert or dialog modal window belonging to an App."""


class App(_WindowBase):
    """Desktop application (primary window / process) linked to a DesktopContext."""

    def _on_closed(self) -> None:
        self._session.unregister_app(self._handle)

    def expect_child_app(
        self,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[App], bool] | None = None,
        timeout: float | None = None,
    ) -> _Expectation[App]:
        return self._dcontext.expect_app(
            title=title,
            pid=pid,
            title_prefix=title_prefix,
            has_text=has_text,
            predicate=predicate,
            timeout=timeout,
        )

    def expect_window(
        self,
        *,
        title: str | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[Window], bool] | None = None,
        timeout: float | None = None,
    ) -> _Expectation[Window]:
        query = WindowQuery.capture(
            title=title, pid=self._pid, title_prefix=title_prefix, has_text=has_text
        )
        validate_predicate(predicate)
        return _Expectation(
            self._session,
            self._dcontext,
            query,
            timeout,
            lambda reply: Window(
                self._session,
                self._dcontext,
                self,
                query,
                handle=reply.handle,
                pid=getattr(reply, "pid", None),
                title=getattr(reply, "title", None),
            ),
            predicate=predicate,
        )

    def expect_alert(
        self,
        *,
        title: str | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[Alert], bool] | None = None,
        timeout: float | None = None,
    ) -> _Expectation[Alert]:
        query = WindowQuery.capture(
            title=title, pid=self._pid, title_prefix=title_prefix, has_text=has_text
        )
        validate_predicate(predicate)
        return _Expectation(
            self._session,
            self._dcontext,
            query,
            timeout,
            lambda reply: Alert(
                self._session,
                self._dcontext,
                self,
                query,
                handle=reply.handle,
                pid=getattr(reply, "pid", None),
                title=getattr(reply, "title", None),
            ),
            predicate=predicate,
        )


class Locator:
    def __init__(self, window: _WindowBase, role: str, name: str, scope=None):
        self._window, self._role, self._name = window, role, name
        self._scope = scope

    def get_by_role(self, role: str, *, name: str) -> Locator:
        validate_control(role, name)
        scope = validate_scope(self._role, self._name, self._scope)
        return Locator(self._window, role, name, scope)

    def _call(self, operation: str, timeout: float | None = None, **parameters):
        if self._scope is not None:
            parameters.update(scope_role=self._scope[0], scope_name=self._scope[1])
        return self._window._call(operation, timeout, role=self._role, name=self._name, **parameters)

    def wait_for(self, *, timeout: float | None = None) -> None:
        self._call("wait_visible", timeout)

    def fill(self, text: str, *, timeout: float | None = None) -> None:
        validate_action_role(self._role, "textbox", "fill")
        validate_text(text)
        self._call("fill", timeout, text=text)

    def input_value(self, *, timeout: float | None = None) -> str:
        validate_action_role(self._role, "textbox", "input_value")
        return self._call("read_text", timeout).text

    def click(self, *, timeout: float | None = None) -> None:
        validate_click_role(self._role)
        self._call("click", timeout)


class EventInfo(Generic[_T]):
    def __init__(self, dcontext: DesktopContext):
        self._dcontext = dcontext
        self._value: _T | None = None

    @property
    def context(self) -> DesktopContext:
        return self._dcontext

    @property
    def value(self) -> _T:
        if self._value is None:
            raise RuntimeError("Read event.value after leaving the expect context.")
        return self._value


class _Expectation(Generic[_T]):
    def __init__(self, session, dcontext: DesktopContext, query: WindowQuery, timeout, factory, *, predicate=None, register=None):
        self._session = session
        self._query = query
        self._timeout = session.timeout(timeout)
        self._factory = factory
        self._predicate = predicate
        self._register = register
        self._info: EventInfo[_T] = EventInfo(dcontext)
        self._capture = None

    def __enter__(self) -> EventInfo[_T]:
        self._deadline = Deadline(self._timeout)
        reply = self._session.call("prepare", **query_parameters(self._query), timeout=self._timeout)
        self._capture = reply.handle
        return self._info

    def __exit__(self, exc_type, exc, traceback):
        if exc_type is not None:
            self._session.cancel_capture(self._capture)
            return False
        with self._session.capture_completion(self._capture, self._deadline) as remaining:
            reply = self._session.call("capture", capture_id=self._capture, timeout=remaining)
            candidate = self._factory(reply)
            self._info._value = accept_captured(candidate, self._predicate, register=self._register)
        return False
