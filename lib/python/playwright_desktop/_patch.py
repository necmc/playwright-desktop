"""Transactional, reversible patch of Playwright's public Page classes."""

import inspect
from importlib.metadata import version
import re
from threading import RLock
import types

from ._errors import DesktopConfigurationError, PatchConflictError

PATCH_OWNER = "playwright_desktop"
PATCH_CONTRACT = 1
PATCHED_ATTRIBUTE = "expect_app"
PLAYWRIGHT_PACKAGE_NAME = "playwright"
MIN_PLAYWRIGHT_VERSION = (1, 63)
MAX_PLAYWRIGHT_VERSION_EXCLUSIVE = (1, 66)
MODE_SYNC = "sync"
MODE_ASYNC = "async"
VERSION_PATTERN = r"(\d+)\.(\d+)\.(\d+)"

_lock = RLock()
_absent = object()


def _page_types():
    from playwright.sync_api import Page as SyncPage
    from playwright.async_api import Page as AsyncPage
    return SyncPage, AsyncPage


def _owned(value):
    return (inspect.getattr_static(value, "_patch_owner", None) == PATCH_OWNER
            and inspect.getattr_static(value, "_patch_contract", None) == PATCH_CONTRACT)


def _facade_factory(mode: str):
    if mode == MODE_SYNC:
        from ._sync import DesktopContext
    else:
        from ._async import DesktopContext
    return DesktopContext


class ExpectAppMethod:
    """Descriptor exposing `page.expect_app(...)` backed by `page.context`'s DesktopContext."""

    _patch_owner = PATCH_OWNER
    _patch_contract = PATCH_CONTRACT

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
        installed = version(PLAYWRIGHT_PACKAGE_NAME)
        match = re.fullmatch(VERSION_PATTERN, installed)
        if not match or not MIN_PLAYWRIGHT_VERSION <= tuple(map(int, match.groups()[:2])) < MAX_PLAYWRIGHT_VERSION_EXCLUSIVE:
            min_ver = ".".join(map(str, MIN_PLAYWRIGHT_VERSION))
            max_ver = ".".join(map(str, MAX_PLAYWRIGHT_VERSION_EXCLUSIVE))
            raise DesktopConfigurationError(
                f"Playwright {installed} is outside the supported range >={min_ver},<{max_ver}."
            )
        classes = _page_types()
        for cls in classes:
            current = inspect.getattr_static(cls, PATCHED_ATTRIBUTE, _absent)
            if current is not _absent and not _owned(current):
                raise PatchConflictError(
                    f"{cls.__module__}.{cls.__name__}.{PATCHED_ATTRIBUTE} is already owned by another extension."
                )
        added = []
        try:
            for cls, mode in zip(classes, (MODE_SYNC, MODE_ASYNC)):
                if inspect.getattr_static(cls, PATCHED_ATTRIBUTE, _absent) is _absent:
                    descriptor = ExpectAppMethod(mode)
                    setattr(cls, PATCHED_ATTRIBUTE, descriptor)
                    added.append((cls, descriptor))
        except BaseException:
            for cls, descriptor in reversed(added):
                if inspect.getattr_static(cls, PATCHED_ATTRIBUTE, _absent) is descriptor:
                    delattr(cls, PATCHED_ATTRIBUTE)
            raise


def uninstall():
    with _lock:
        for cls in _page_types():
            if _owned(inspect.getattr_static(cls, PATCHED_ATTRIBUTE, _absent)):
                delattr(cls, PATCHED_ATTRIBUTE)
