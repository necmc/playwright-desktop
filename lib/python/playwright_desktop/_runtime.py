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
    NATIVE_ERRORS,
)
from ._models import validate_timeout
from ._timing import DEFAULT_TIMEOUT_MS, Deadline

_BINDING_TIMEOUT = "The desktop request timed out while initializing its binding."

_lock = Lock()
_engine = None
_native_error = None

_config_lock = Lock()
_default_java_home: str | None = None
_default_timeout: float = DEFAULT_TIMEOUT_MS

_sessions_lock = Lock()
_sessions: weakref.WeakKeyDictionary[object, "Session"] = weakref.WeakKeyDictionary()
_facades: weakref.WeakKeyDictionary["Session", dict[str, object]] = weakref.WeakKeyDictionary()


def configure(*, java_home=None, timeout: float | None = None) -> None:
    """Configure defaults applied to newly opened BrowserContext-scoped desktop sessions."""
    global _default_java_home, _default_timeout
    if java_home is None and timeout is None:
        raise ValueError("Specify java_home and/or timeout.")
    normalized_java = _backend.normalize_java_home(java_home) if java_home is not None else None
    validated_timeout = validate_timeout(timeout) if timeout is not None else None
    with _config_lock:
        if normalized_java is not None:
            _default_java_home = normalized_java
        if validated_timeout is not None:
            _default_timeout = validated_timeout


def reset_defaults() -> None:
    global _default_java_home, _default_timeout
    with _config_lock:
        _default_java_home = None
        _default_timeout = DEFAULT_TIMEOUT_MS


def session_for_context(browser_context) -> "Session":
    """Return the active desktop Session correlated 1:1 with a Playwright BrowserContext."""
    if hasattr(browser_context, "is_closed") and browser_context.is_closed():
        raise DesktopClosedError("The parent Playwright BrowserContext is closed.")
    with _sessions_lock:
        session = _sessions.get(browser_context)
        if session is None or session.is_closed():
            session = Session(browser_context)
            _sessions[browser_context] = session
        return session


def context_facade(session: "Session", mode: str, factory):
    """Return a single DesktopContext facade per (Session, mode) without strong context cycles."""
    with _sessions_lock:
        per_mode = _facades.setdefault(session, {})
        facade = per_mode.get(mode)
        if facade is None:
            facade = factory(session)
            per_mode[mode] = facade
        return facade


def _get_engine():
    global _engine, _native_error
    with _lock:
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
            code, message = error.args if len(error.args) == 2 else ("ACTION", str(error))
            raise NATIVE_ERRORS.get(code, DesktopError)(message) from error
        raise


def _invoke(engine, request, operation, parameters):
    return _native_call(engine.invoke, request, operation, **parameters)


class Session:
    def __init__(self, browser_context):
        self._browser_context = weakref.ref(browser_context)
        self._thread = get_ident()
        with _config_lock:
            self._java_home = _default_java_home
            self.default_timeout = _default_timeout
        self._engine = None
        self._id = None
        self._closed = False
        self._apps: dict[int, weakref.ref] = {}
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

    def register_app(self, handle: int, app) -> None:
        self._apps[handle] = weakref.ref(app)

    def unregister_app(self, handle: int) -> None:
        self._apps.pop(handle, None)

    def tracked_apps(self) -> tuple:
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
            self._apps.clear()
            context = self._browser_context()
            if context is not None:
                with _sessions_lock:
                    if _sessions.get(context) is self:
                        del _sessions[context]
                    _facades.pop(self, None)
            if self._engine is not None:
                self._engine.close_session(self._id)
