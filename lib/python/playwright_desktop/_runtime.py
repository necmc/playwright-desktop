"""Lazy binding access and BrowserContext-scoped desktop session lifecycle."""

import asyncio
from contextlib import contextmanager
from threading import Lock, get_ident
import weakref

from . import _backend
from ._errors import (
    DesktopClosedError,
    DesktopConfigurationError,
    DesktopError,
    DesktopTimeoutError,
    translate_native_error,
)
from ._models import InvokeParams, validate_timeout
from ._timing import DEFAULT_TIMEOUT_MS, Deadline

_BINDING_TIMEOUT = "The desktop request timed out while initializing its binding."

_engine_lock = Lock()
_engine = None
_native_error = None


class _Defaults:
    def __init__(self):
        self._lock = Lock()
        self._java_home: str | None = None
        self._timeout: float = DEFAULT_TIMEOUT_MS

    def configure(self, *, java_home=None, timeout: float | None = None) -> None:
        if java_home is None and timeout is None:
            raise ValueError("Specify java_home and/or timeout.")
        normalized_java = _backend.normalize_java_home(java_home) if java_home is not None else None
        validated_timeout = validate_timeout(timeout) if timeout is not None else None
        with self._lock:
            if normalized_java is not None:
                self._java_home = normalized_java
            if validated_timeout is not None:
                self._timeout = validated_timeout

    def reset(self) -> None:
        with self._lock:
            self._java_home = None
            self._timeout = DEFAULT_TIMEOUT_MS

    def snapshot(self) -> tuple[str | None, float]:
        with self._lock:
            return self._java_home, self._timeout


class _SessionRegistry:
    def __init__(self):
        self._lock = Lock()
        self._sessions: weakref.WeakKeyDictionary[object, "Session"] = weakref.WeakKeyDictionary()
        self._facades: weakref.WeakKeyDictionary["Session", dict[str, object]] = weakref.WeakKeyDictionary()

    def session_for_context(self, browser_context) -> "Session":
        if hasattr(browser_context, "is_closed") and browser_context.is_closed():
            raise DesktopClosedError("The parent Playwright BrowserContext is closed.")
        with self._lock:
            session = self._sessions.get(browser_context)
            if session is None or session.is_closed():
                session = Session(browser_context)
                self._sessions[browser_context] = session
            return session

    def context_facade(self, session: "Session", mode: str, factory):
        with self._lock:
            per_mode = self._facades.setdefault(session, {})
            facade = per_mode.get(mode)
            if facade is None:
                facade = factory(session)
                per_mode[mode] = facade
            return facade

    def discard(self, browser_context, session: "Session") -> None:
        with self._lock:
            if browser_context is not None and self._sessions.get(browser_context) is session:
                del self._sessions[browser_context]
            self._facades.pop(session, None)


_defaults = _Defaults()
_registry = _SessionRegistry()


def configure(*, java_home=None, timeout: float | None = None) -> None:
    """Configure defaults applied to newly opened BrowserContext-scoped desktop sessions."""
    _defaults.configure(java_home=java_home, timeout=timeout)


def reset_defaults() -> None:
    _defaults.reset()


def session_for_context(browser_context) -> "Session":
    """Return the active desktop Session correlated 1:1 with a Playwright BrowserContext."""
    return _registry.session_for_context(browser_context)


def context_facade(session: "Session", mode: str, factory):
    """Return a single DesktopContext facade per (Session, mode) without strong context cycles."""
    return _registry.context_facade(session, mode, factory)


def _get_engine():
    global _engine, _native_error
    with _engine_lock:
        if _engine is None:
            native = _backend.load_extension()
            _native_error = native.NativeError
            _engine = _native_call(native.Engine)
        return _engine


def _native_call(function, *args, **kwargs):
    try:
        return function(*args, **kwargs)
    except Exception as error:
        if _native_error is not None and isinstance(error, _native_error):
            raise translate_native_error(error) from error
        raise


def _invoke(engine, request, operation, parameters):
    return _native_call(engine.invoke, request, InvokeParams(operation=operation, **parameters))


class Session:
    def __init__(self, browser_context):
        self._browser_context = weakref.ref(browser_context)
        self._thread = get_ident()
        self._java_home, self.default_timeout = _defaults.snapshot()
        self._engine = None
        self._id = None
        self._closed = False
        owner = weakref.ref(self)

        def on_close(*_):
            session = owner()
            if session is not None:
                session.close_nowait()

        browser_context.on("close", on_close)
        self._finalizer = weakref.finalize(browser_context, self.close_nowait)

    @property
    def browser_context(self):
        context = self._browser_context()
        if context is None or (hasattr(context, "is_closed") and context.is_closed()):
            raise DesktopClosedError("The parent Playwright BrowserContext is closed.")
        return context

    def is_closed(self) -> bool:
        context = self._browser_context()
        return self._closed or context is None or (hasattr(context, "is_closed") and context.is_closed())

    def _check_owner(self):
        if get_ident() != self._thread:
            raise DesktopError("Use this DesktopContext on the same thread as its Playwright BrowserContext.")
        if self.is_closed():
            raise DesktopClosedError("The DesktopContext or its Playwright BrowserContext is closed.")

    def configure(self, *, java_home):
        self._check_owner()
        if self._id is not None:
            raise DesktopConfigurationError("Configure the Java runtime before the first desktop operation.")
        self._java_home = _backend.normalize_java_home(java_home)

    def set_default_timeout(self, timeout):
        self._check_owner()
        self.default_timeout = validate_timeout(timeout)

    def timeout(self, timeout):
        return self.default_timeout if timeout is None else validate_timeout(timeout)

    def _open_session(self, engine, backend_argument: str | None) -> None:
        self._id = _native_call(engine.open_auto_session, backend_argument, explicit=self._java_home is not None)
        self._engine = engine

    def _new_request(self, deadline: Deadline):
        remaining = deadline.remaining_ms(_BINDING_TIMEOUT)
        return _native_call(self._engine.new_request, self._id, remaining)

    def call(self, operation, *, timeout=None, **parameters):
        self._check_owner()
        deadline = Deadline(self.timeout(timeout))
        if self._id is None:
            backend_argument = _backend.session_argument(self._java_home)
            self._open_session(_get_engine(), backend_argument)
        request = self._new_request(deadline)
        return _invoke(self._engine, request, operation, parameters)

    async def call_async(self, operation, *, timeout=None, **parameters):
        self._check_owner()
        deadline = Deadline(self.timeout(timeout))
        if self._id is None:
            backend_argument = _backend.session_argument(self._java_home)
            startup_budget = deadline.remaining_ms(_BINDING_TIMEOUT) or DEFAULT_TIMEOUT_MS
            try:
                engine = await asyncio.wait_for(asyncio.to_thread(_get_engine), startup_budget / 1000)
            except asyncio.TimeoutError as error:
                raise DesktopTimeoutError("The native binding did not initialize before the deadline.") from error
            self._check_owner()
            deadline.remaining_ms(_BINDING_TIMEOUT)
            # Another coroutine can open this session while the engine starts.
            if self._id is None:
                self._open_session(engine, backend_argument)
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
        if self._engine is not None and not self._closed:
            self._engine.cancel_capture(self._id, capture_id)

    def close_nowait(self):
        if not self._closed:
            self._closed = True
            _registry.discard(self._browser_context(), self)
            if self._engine is not None:
                self._engine.close_session(self._id)
