"""Transactional, reversible patch of Playwright's public Page classes."""

import inspect
from importlib.metadata import version
import re
from threading import RLock
import types

from ._errors import DesktopConfigurationError, PatchConflictError

_lock = RLock()
_absent = object()
_ATTR = "expect_app"


def _page_types():
    from playwright.sync_api import Page as SyncPage
    from playwright.async_api import Page as AsyncPage
    return SyncPage, AsyncPage


def _owned(value):
    return (inspect.getattr_static(value, "_patch_owner", None) == "playwright_desktop"
            and inspect.getattr_static(value, "_patch_contract", None) == 1)


def _facade_factory(mode: str):
    if mode == "sync":
        from ._sync import DesktopContext
    else:
        from ._async import DesktopContext
    return DesktopContext


class ExpectAppMethod:
    """Descriptor exposing `page.expect_app(...)` backed by `page.context`'s DesktopContext."""

    _patch_owner = "playwright_desktop"
    _patch_contract = 1

    def __init__(self, mode: str):
        self._mode = mode

    def _call(self, page, *args, **kwargs):
        from ._runtime import context_facade, session_for_context

        session = session_for_context(page.context)
        dcontext = context_facade(session, self._mode, _facade_factory(self._mode))
        return dcontext.expect_app(*args, **kwargs)

    def __get__(self, page, owner=None):
        if page is None:
            return self
        return types.MethodType(self._call, page)


def install():
    with _lock:
        installed = version("playwright")
        match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", installed)
        if not match or not (1, 63) <= tuple(map(int, match.groups()[:2])) < (1, 66):
            raise DesktopConfigurationError(f"Playwright {installed} is outside the supported range >=1.63,<1.66.")
        classes = _page_types()
        for cls in classes:
            current = inspect.getattr_static(cls, _ATTR, _absent)
            if current is not _absent and not _owned(current):
                raise PatchConflictError(f"{cls.__module__}.{cls.__name__}.{_ATTR} is already owned by another extension.")
        added = []
        try:
            for cls, mode in zip(classes, ("sync", "async")):
                if inspect.getattr_static(cls, _ATTR, _absent) is _absent:
                    descriptor = ExpectAppMethod(mode)
                    setattr(cls, _ATTR, descriptor)
                    added.append((cls, descriptor))
        except BaseException:
            for cls, descriptor in reversed(added):
                if inspect.getattr_static(cls, _ATTR, _absent) is descriptor:
                    delattr(cls, _ATTR)
            raise


def uninstall():
    with _lock:
        for cls in _page_types():
            if _owned(inspect.getattr_static(cls, _ATTR, _absent)):
                delattr(cls, _ATTR)
