"""Async native desktop API; Playwright calls remain on their original loop."""

from __future__ import annotations

from os import PathLike

from ._models import (
    WindowQuery, validate_action_role, validate_click_role, validate_control, validate_scope, validate_text,
)
from ._timing import Deadline


class Desktop:
    def __init__(self, session):
        self._session = session

    def configure(self, *, java_home: str | PathLike[str]) -> None:
        self._session.configure(java_home=java_home)

    def set_default_timeout(self, timeout: float) -> None:
        self._session.set_default_timeout(timeout)

    def window(self, *, title: str, pid: int | None = None) -> Window:
        return Window(self._session, WindowQuery(title, pid))

    def expect_window(self, *, title: str, pid: int | None = None, timeout: float | None = None) -> WindowExpectation:
        return WindowExpectation(self._session, WindowQuery(title, pid), timeout)

    def get_by_role(self, role: str, *, name: str) -> Locator:
        return self._captured_window().get_by_role(role, name=name)

    async def wait_for_closed(self, *, timeout: float | None = None) -> None:
        await self._captured_window().wait_for_closed(timeout=timeout)

    def _captured_window(self) -> Window:
        query, handle = self._session.require_captured_window()
        return Window(self._session, query, handle)

    async def close(self) -> None:
        self._session.close_nowait()


class Window:
    def __init__(self, session, query, handle=0):
        self._session, self._query, self._handle = session, query, handle

    def get_by_role(self, role: str, *, name: str) -> Locator:
        validate_control(role, name)
        return Locator(self, role, name)

    async def _call(self, operation, timeout=None, **parameters):
        reply = await self._session.call_async(operation, window_id=self._handle,
                                              title=self._query.title, pid=self._query.pid,
                                              timeout=timeout, **parameters)
        if reply.handle:
            self._handle = reply.handle
        return reply

    async def wait_for_closed(self, *, timeout: float | None = None) -> None:
        await self._call("wait_closed", timeout)


class Locator:
    def __init__(self, window, role, name, scope=None):
        self._window, self._role, self._name = window, role, name
        self._scope = scope

    def get_by_role(self, role: str, *, name: str) -> Locator:
        validate_control(role, name)
        scope = validate_scope(self._role, self._name, self._scope)
        return Locator(self._window, role, name, scope)

    async def _call(self, operation, timeout=None, **parameters):
        if self._scope is not None:
            parameters.update(scope_role=self._scope[0], scope_name=self._scope[1])
        return await self._window._call(operation, timeout, role=self._role, name=self._name, **parameters)

    async def fill(self, text: str, *, timeout: float | None = None) -> None:
        validate_action_role(self._role, "textbox", "fill")
        validate_text(text)
        await self._call("fill", timeout, text=text)

    async def input_value(self, *, timeout: float | None = None) -> str:
        validate_action_role(self._role, "textbox", "input_value")
        return (await self._call("read_text", timeout)).text

    async def click(self, *, timeout: float | None = None) -> None:
        validate_click_role(self._role)
        await self._call("click", timeout)


class EventInfo:
    def __init__(self):
        self._value = None

    @property
    def value(self):
        async def result():
            if self._value is None:
                raise RuntimeError("Await opened.value after leaving the expect_window context.")
            return self._value
        return result()


class WindowExpectation:
    def __init__(self, session, query, timeout):
        self._session, self._query = session, query
        self._timeout = session.timeout(timeout)
        self._info = EventInfo()
        self._capture = None

    async def __aenter__(self):
        self._deadline = Deadline(self._timeout)
        reply = await self._session.call_async("prepare", title=self._query.title,
                                               pid=self._query.pid, timeout=self._timeout)
        self._capture = reply.handle
        return self._info

    async def __aexit__(self, exc_type, exc, traceback):
        if exc_type is not None:
            self._session.cancel_capture(self._capture)
            return False
        with self._session.capture_completion(self._capture, self._deadline) as remaining:
            reply = await self._session.call_async("capture", capture_id=self._capture, timeout=remaining)
            self._session.bind_captured_window(self._query, reply.handle)
            self._info._value = Window(self._session, self._query, reply.handle)
        return False
