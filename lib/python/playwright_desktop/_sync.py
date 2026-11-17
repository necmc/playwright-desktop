"""Synchronous native desktop API."""

from __future__ import annotations

import math
from os import PathLike
from time import monotonic

from ._errors import DesktopTimeoutError
from ._models import WindowQuery, validate_control, validate_text


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

    def close(self) -> None:
        self._session.close_nowait()


class Window:
    def __init__(self, session, query, handle=0):
        self._session, self._query, self._handle = session, query, handle

    def get_by_role(self, role: str, *, name: str) -> Locator:
        validate_control(role, name)
        return Locator(self, role, name)

    def _call(self, operation, timeout=None, **parameters):
        reply = self._session.call(operation, window_id=self._handle, title=self._query.title,
                                   pid=self._query.pid, timeout=timeout, **parameters)
        if reply.handle:
            self._handle = reply.handle
        return reply

    def wait_for_closed(self, *, timeout: float | None = None) -> None:
        self._call("wait_closed", timeout)


class Locator:
    def __init__(self, window, role, name):
        self._window, self._role, self._name = window, role, name

    def _call(self, operation, timeout=None, **parameters):
        return self._window._call(operation, timeout, role=self._role, name=self._name, **parameters)

    def fill(self, text: str, *, timeout: float | None = None) -> None:
        if self._role != "textbox":
            raise ValueError("fill() requires the textbox role.")
        validate_text(text)
        self._call("fill", timeout, text=text)

    def input_value(self, *, timeout: float | None = None) -> str:
        if self._role != "textbox":
            raise ValueError("input_value() requires the textbox role.")
        return self._call("read_text", timeout).text

    def click(self, *, timeout: float | None = None) -> None:
        if self._role != "button":
            raise ValueError("click() requires the button role.")
        self._call("click", timeout)


class EventInfo:
    def __init__(self):
        self._value = None

    @property
    def value(self):
        if self._value is None:
            raise RuntimeError("Read opened.value after leaving the expect_window context.")
        return self._value


class WindowExpectation:
    def __init__(self, session, query, timeout):
        self._session, self._query = session, query
        self._timeout = session.timeout(timeout)
        self._info = EventInfo()
        self._capture = None

    def __enter__(self):
        self._deadline = monotonic() + self._timeout / 1000
        reply = self._session.call("prepare", title=self._query.title, pid=self._query.pid, timeout=self._timeout)
        self._capture = reply.handle
        return self._info

    def __exit__(self, exc_type, exc, traceback):
        if exc_type is not None:
            self._session.cancel_capture(self._capture)
            return False
        try:
            remaining = max(0, math.ceil((self._deadline - monotonic()) * 1000))
            if self._timeout and not remaining:
                raise DesktopTimeoutError("The desktop window capture timed out during the web action.")
            reply = self._session.call("capture", capture_id=self._capture, timeout=remaining)
            self._info._value = Window(self._session, self._query, reply.handle)
        except BaseException:
            self._session.cancel_capture(self._capture)
            raise
        return False
