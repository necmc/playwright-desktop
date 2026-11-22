"""Async native desktop API aligned with Playwright BrowserContext/Page."""

from __future__ import annotations

from typing import Callable, Generic, TypeVar

from ._facade import _DesktopContextBase, _WindowBase, _accept_captured
from ._models import (
    OP_BRING_TO_FRONT,
    OP_CAPTURE,
    OP_CLICK,
    OP_CLOSE_WINDOW,
    OP_FILL,
    OP_PREPARE,
    OP_READ_TEXT,
    OP_WAIT_CLOSED,
    OP_WAIT_VISIBLE,
    ROLE_TEXTBOX,
    WindowQuery,
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


class DesktopContext(_DesktopContextBase):
    """Async desktop environment correlated 1:1 with a Playwright BrowserContext."""

    def app(
        self,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        only_new: bool = False,
    ) -> App:
        return App(self._session, self, WindowQuery(title, pid, title_prefix, has_text, only_new=only_new))

    def expect_app(
        self,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[App], bool] | None = None,
        timeout: float | None = None,
    ) -> _AsyncExpectation[App]:
        query = WindowQuery.capture(title=title, pid=pid, title_prefix=title_prefix, has_text=has_text)
        validate_predicate(predicate)
        return _AsyncExpectation(
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
            register=self._register_app,
        )

    async def close(self) -> None:
        self._clear_apps()
        self._session.close_nowait()


class _AsyncWindow(_WindowBase):
    def get_by_role(self, role: str, *, name: str) -> Locator:
        validate_control(role, name)
        return Locator(self, role, name)

    async def _call(self, operation: str, timeout: float | None = None, **parameters):
        reply = await self._session.call_async(
            operation,
            window_id=self._handle,
            **query_parameters(self._query),
            timeout=timeout,
            **parameters,
        )
        self._apply_reply(reply)
        return reply

    async def bring_to_front(self, *, timeout: float | None = None) -> None:
        await self._call(OP_BRING_TO_FRONT, timeout)

    async def wait_for_closed(self, *, timeout: float | None = None) -> None:
        await self._call(OP_WAIT_CLOSED, timeout)

    def _on_closed(self) -> None:
        pass

    async def close(self, *, timeout: float | None = None) -> None:
        await self._call(OP_CLOSE_WINDOW, timeout)
        self._on_closed()


class Window(_AsyncWindow):
    """Secondary or modal window belonging to an App."""

    def __init__(self, session, dcontext: DesktopContext, app: App, query: WindowQuery, handle: int = 0, *, pid=None, title=None):
        super().__init__(session, dcontext, query, handle, pid=pid, title=title)
        self._app = app

    @property
    def app(self) -> App:
        return self._app


class Alert(Window):
    """Alert or dialog modal window belonging to an App."""


class App(_AsyncWindow):
    """Desktop application (primary window / process) linked to a DesktopContext."""

    def _on_closed(self) -> None:
        self._dcontext._unregister_app(self._handle)

    def expect_child_app(
        self,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[App], bool] | None = None,
        timeout: float | None = None,
    ) -> _AsyncExpectation[App]:
        return self._dcontext.expect_app(
            title=title,
            pid=pid,
            title_prefix=title_prefix,
            has_text=has_text,
            predicate=predicate,
            timeout=timeout,
        )

    def _expect_owned_window(
        self,
        window_cls: type[_T],
        *,
        title: str | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[_T], bool] | None = None,
        timeout: float | None = None,
    ) -> _AsyncExpectation[_T]:
        query = WindowQuery.capture(
            title=title, pid=self._pid, title_prefix=title_prefix, has_text=has_text
        )
        validate_predicate(predicate)
        return _AsyncExpectation(
            self._session,
            self._dcontext,
            query,
            timeout,
            lambda reply: window_cls(
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

    def expect_window(
        self,
        *,
        title: str | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[Window], bool] | None = None,
        timeout: float | None = None,
    ) -> _AsyncExpectation[Window]:
        return self._expect_owned_window(
            Window,
            title=title,
            title_prefix=title_prefix,
            has_text=has_text,
            predicate=predicate,
            timeout=timeout,
        )

    def expect_alert(
        self,
        *,
        title: str | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[Alert], bool] | None = None,
        timeout: float | None = None,
    ) -> _AsyncExpectation[Alert]:
        return self._expect_owned_window(
            Alert,
            title=title,
            title_prefix=title_prefix,
            has_text=has_text,
            predicate=predicate,
            timeout=timeout,
        )


class Locator:
    def __init__(self, window: _AsyncWindow, role: str, name: str, scope=None):
        self._window, self._role, self._name = window, role, name
        self._scope = scope

    def get_by_role(self, role: str, *, name: str) -> Locator:
        validate_control(role, name)
        scope = validate_scope(self._role, self._name, self._scope)
        return Locator(self._window, role, name, scope)

    async def _call(self, operation: str, timeout: float | None = None, **parameters):
        if self._scope is not None:
            parameters.update(scope_role=self._scope[0], scope_name=self._scope[1])
        return await self._window._call(operation, timeout, role=self._role, name=self._name, **parameters)

    async def wait_for(self, *, timeout: float | None = None) -> None:
        await self._call(OP_WAIT_VISIBLE, timeout)

    async def fill(self, text: str, *, timeout: float | None = None) -> None:
        validate_action_role(self._role, ROLE_TEXTBOX, OP_FILL)
        validate_text(text)
        await self._call(OP_FILL, timeout, text=text)

    async def input_value(self, *, timeout: float | None = None) -> str:
        validate_action_role(self._role, ROLE_TEXTBOX, "input_value")
        return (await self._call(OP_READ_TEXT, timeout)).text

    async def click(self, *, timeout: float | None = None) -> None:
        validate_click_role(self._role)
        await self._call(OP_CLICK, timeout)


class AsyncEventInfo(Generic[_T]):
    def __init__(self, dcontext: DesktopContext):
        self._dcontext = dcontext
        self._value: _T | None = None

    @property
    def context(self) -> DesktopContext:
        return self._dcontext

    @property
    def value(self):
        async def result() -> _T:
            if self._value is None:
                raise RuntimeError("Await event.value after leaving the expect context.")
            return self._value
        return result()


class _AsyncExpectation(Generic[_T]):
    def __init__(self, session, dcontext: DesktopContext, query: WindowQuery, timeout, factory, *, predicate=None, register=None):
        self._session = session
        self._query = query
        self._timeout = session.timeout(timeout)
        self._factory = factory
        self._predicate = predicate
        self._register = register
        self._info: AsyncEventInfo[_T] = AsyncEventInfo(dcontext)
        self._capture = None

    async def __aenter__(self) -> AsyncEventInfo[_T]:
        self._deadline = Deadline(self._timeout)
        reply = await self._session.call_async(OP_PREPARE, **query_parameters(self._query), timeout=self._timeout)
        self._capture = reply.handle
        return self._info

    async def __aexit__(self, exc_type, exc, traceback):
        if exc_type is not None:
            self._session.cancel_capture(self._capture)
            return False
        with self._session.capture_completion(self._capture, self._deadline) as remaining:
            reply = await self._session.call_async(OP_CAPTURE, capture_id=self._capture, timeout=remaining)
            candidate = self._factory(reply)
            self._info._value = _accept_captured(candidate, self._predicate, register=self._register)
        return False
