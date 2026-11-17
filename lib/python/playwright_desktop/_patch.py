"""Transactional, reversible patch of Playwright's public Page classes."""

import inspect
from importlib.metadata import version
import re
from threading import RLock
from weakref import WeakKeyDictionary

from ._errors import DesktopConfigurationError, PatchConflictError

_lock = RLock()
_absent = object()


def _page_types():
    from playwright.sync_api import Page as SyncPage
    from playwright.async_api import Page as AsyncPage
    return SyncPage, AsyncPage


def _owned(value):
    return (inspect.getattr_static(value, "_patch_owner", None) == "playwright_desktop"
            and inspect.getattr_static(value, "_patch_contract", None) == 1)


class DesktopAccessor:
    _patch_owner = "playwright_desktop"
    _patch_contract = 1

    def __init__(self, mode):
        self._mode = mode
        self._facades = WeakKeyDictionary()
        self._lock = RLock()

    def __get__(self, page, owner=None):
        if page is None:
            return self
        with self._lock:
            if page not in self._facades:
                from ._runtime import Session
                if self._mode == "sync":
                    from ._sync import Desktop
                else:
                    from ._async import Desktop
                self._facades[page] = Desktop(Session(page))
            return self._facades[page]


def install():
    with _lock:
        installed = version("playwright")
        match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)", installed)
        if not match or not (1, 63) <= tuple(map(int, match.groups()[:2])) < (1, 66):
            raise DesktopConfigurationError(f"Playwright {installed} is outside the supported range >=1.63,<1.66.")
        classes = _page_types()
        for cls in classes:
            current = inspect.getattr_static(cls, "desktop", _absent)
            if current is not _absent and not _owned(current):
                raise PatchConflictError(f"{cls.__module__}.{cls.__name__}.desktop is already owned by another extension.")
        added = []
        try:
            for cls, mode in zip(classes, ("sync", "async")):
                if inspect.getattr_static(cls, "desktop", _absent) is _absent:
                    descriptor = DesktopAccessor(mode)
                    setattr(cls, "desktop", descriptor)
                    added.append((cls, descriptor))
        except BaseException:
            for cls, descriptor in reversed(added):
                if inspect.getattr_static(cls, "desktop", _absent) is descriptor:
                    delattr(cls, "desktop")
            raise


def uninstall():
    with _lock:
        for cls in _page_types():
            if _owned(inspect.getattr_static(cls, "desktop", _absent)):
                delattr(cls, "desktop")
