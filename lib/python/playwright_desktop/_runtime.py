"""Lazy binding access. Playwright objects never cross the native worker boundary."""

import asyncio
from contextlib import contextmanager
import importlib
import os
from pathlib import Path
import struct
import sys
import sysconfig
from threading import Lock, get_ident
import weakref

from ._errors import (
    DesktopClosedError,
    DesktopConfigurationError,
    DesktopError,
    DesktopTimeoutError,
    NATIVE_ERRORS,
    UnsupportedPlatformError,
)
from ._models import WindowQuery, validate_timeout
from ._timing import DEFAULT_TIMEOUT_MS, Deadline

_BINDING_TIMEOUT = "The desktop request timed out while initializing its binding."

_lock = Lock()
_engine = None
_native_error = None


def _get_engine():
    global _engine, _native_error
    with _lock:
        if _engine is None:
            if sys.platform != "win32" or struct.calcsize("P") != 8 or sysconfig.get_config_var("Py_GIL_DISABLED"):
                raise UnsupportedPlatformError("The native backend requires conventional CPython on Windows x64.")
            try:
                native = importlib.import_module("playwright_desktop._native")
            except ImportError as error:
                raise DesktopError("The native extension is not installed. Build and install the playwright-desktop wheel.") from error
            _native_error = native.NativeError
            _engine = _native_call(native.Engine)
        return _engine


def _native_call(function, *args, **kwargs):
    try:
        return function(*args, **kwargs)
    except Exception as error:
        if _native_error is not None and isinstance(error, _native_error):
            code, message = error.args if len(error.args) == 2 else ("ACTION", str(error))
            raise NATIVE_ERRORS.get(code, DesktopError)(message) from error
        raise


def _invoke(engine, request, operation, parameters):
    return _native_call(engine.invoke, request, operation, **parameters)


class Session:
    def __init__(self, page):
        self._page = weakref.ref(page)
        self._thread = get_ident()
        self._java_home = None
        self._engine = None
        self._id = None
        self._closed = False
        self._captured_window: tuple[WindowQuery, int] | None = None
        self.default_timeout = DEFAULT_TIMEOUT_MS
        owner = weakref.ref(self)

        def on_close(*_):
            session = owner()
            if session is not None:
                session.close_nowait()

        page.on("close", on_close)
        self._finalizer = weakref.finalize(page, self.close_nowait)

    def _check_owner(self):
        if get_ident() != self._thread:
            raise DesktopError("Use this desktop facade on the same thread as its Playwright Page.")
        page = self._page()
        if self._closed or page is None or page.is_closed():
            raise DesktopClosedError("The desktop session or its Playwright Page is closed.")

    def configure(self, *, java_home):
        self._check_owner()
        if self._id is not None:
            raise DesktopConfigurationError("Configure the Java runtime before the first desktop operation.")
        path = os.fspath(java_home)
        if not isinstance(path, str) or not path or "\0" in path:
            raise ValueError("java_home must be a nonempty filesystem path.")
        self._java_home = str(Path(path).expanduser().absolute())

    def set_default_timeout(self, timeout):
        self._check_owner()
        self.default_timeout = validate_timeout(timeout)

    def timeout(self, timeout):
        return self.default_timeout if timeout is None else validate_timeout(timeout)

    def bind_captured_window(self, query: WindowQuery, handle: int) -> None:
        self._check_owner()
        self._captured_window = (query, handle)

    def require_captured_window(self) -> tuple[WindowQuery, int]:
        self._check_owner()
        if self._captured_window is None:
            raise DesktopConfigurationError("Capture a window with page.desktop.expect_window() first.")
        return self._captured_window

    def _runtime_path(self) -> str:
        if self._java_home is not None:
            return self._java_home
        if java_home := os.environ.get("JAVA_HOME"):
            return str(Path(java_home).expanduser().absolute())
        raise DesktopConfigurationError("Set JAVA_HOME or call page.desktop.configure(java_home=...).")

    def _open_session(self, engine, java_home: str) -> None:
        self._id = _native_call(engine.open_session, java_home)
        self._engine = engine

    def _new_request(self, deadline: Deadline):
        remaining = deadline.remaining_ms(_BINDING_TIMEOUT)
        return _native_call(self._engine.new_request, self._id, remaining)

    def call(self, operation, *, timeout=None, **parameters):
        self._check_owner()
        deadline = Deadline(self.timeout(timeout))
        if self._id is None:
            java_home = self._runtime_path()
            self._open_session(_get_engine(), java_home)
        request = self._new_request(deadline)
        return _invoke(self._engine, request, operation, parameters)

    async def call_async(self, operation, *, timeout=None, **parameters):
        self._check_owner()
        deadline = Deadline(self.timeout(timeout))
        if self._id is None:
            java_home = self._runtime_path()
            startup_budget = deadline.remaining_ms(_BINDING_TIMEOUT) or DEFAULT_TIMEOUT_MS
            try:
                engine = await asyncio.wait_for(asyncio.to_thread(_get_engine), startup_budget / 1000)
            except asyncio.TimeoutError as error:
                raise DesktopTimeoutError("The native binding did not initialize before the deadline.") from error
            self._check_owner()
            deadline.remaining_ms(_BINDING_TIMEOUT)
            # Another coroutine can open this session while the engine starts.
            if self._id is None:
                self._open_session(engine, java_home)
        request = self._new_request(deadline)
        try:
            return await asyncio.to_thread(_invoke, self._engine, request, operation, parameters)
        except asyncio.CancelledError:
            request.cancel()
            raise

    @contextmanager
    def capture_completion(self, capture_id: int, deadline: Deadline):
        try:
            yield deadline.remaining_ms("The desktop window capture timed out during the web action.")
        except BaseException:
            self.cancel_capture(capture_id)
            raise

    def cancel_capture(self, capture_id):
        if self._engine is not None:
            self._engine.cancel_capture(self._id, capture_id)

    def close_nowait(self):
        if not self._closed:
            self._closed = True
            self._captured_window = None
            if self._engine is not None:
                self._engine.close_session(self._id)
