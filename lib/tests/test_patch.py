import gc
import importlib
import inspect
import sys
import subprocess
import unittest
import weakref
from unittest.mock import patch

from playwright.sync_api import Page as SyncPage
from playwright.async_api import Page as AsyncPage

import playwright_desktop as desktop
from playwright_desktop import _patch
from playwright_desktop._errors import PatchConflictError


class FakePage:
    def __init__(self):
        self.callbacks = {}

    def on(self, event, callback):
        self.callbacks[event] = callback

    def is_closed(self):
        return False


class PatchTests(unittest.TestCase):
    def setUp(self):
        desktop.uninstall()

    def tearDown(self):
        desktop.uninstall()
        desktop.install()

    def test_import_patches_both_official_classes_without_replacing_methods(self):
        originals = {cls: {name: getattr(cls, name) for name in ("goto", "click", "locator", "close")}
                     for cls in (SyncPage, AsyncPage)}
        desktop.install()
        for cls, methods in originals.items():
            self.assertIn("desktop", vars(cls))
            for name, method in methods.items():
                self.assertIs(getattr(cls, name), method)

    def test_install_and_reload_are_idempotent_and_native_import_is_lazy(self):
        desktop.install()
        descriptor = inspect.getattr_static(SyncPage, "desktop")
        native_loaded = "playwright_desktop._native" in sys.modules
        desktop.install()
        importlib.reload(desktop)
        self.assertIs(inspect.getattr_static(SyncPage, "desktop"), descriptor)
        self.assertEqual("playwright_desktop._native" in sys.modules, native_loaded)
        subprocess.run([sys.executable, "-c", "import sys; import playwright_desktop; assert 'playwright_desktop._native' not in sys.modules"], check=True)

    def test_collision_is_detected_before_either_class_is_patched(self):
        marker = object()
        setattr(AsyncPage, "desktop", marker)
        try:
            with self.assertRaises(PatchConflictError):
                desktop.install()
            self.assertNotIn("desktop", vars(SyncPage))
            self.assertIs(inspect.getattr_static(AsyncPage, "desktop"), marker)
        finally:
            delattr(AsyncPage, "desktop")

    def test_failed_second_assignment_rolls_back_the_first(self):
        class Reject(type):
            def __setattr__(cls, name, value):
                if name == "desktop":
                    raise RuntimeError("injected assignment failure")
                super().__setattr__(name, value)

        class Second(metaclass=Reject):
            pass

        with patch.object(_patch, "_page_types", return_value=(FakePage, Second)):
            with self.assertRaises(RuntimeError):
                desktop.install()
            self.assertNotIn("desktop", vars(FakePage))

    def test_uninstall_preserves_a_replacement_owned_by_someone_else(self):
        desktop.install()
        marker = object()
        setattr(SyncPage, "desktop", marker)
        try:
            desktop.uninstall()
            self.assertIs(inspect.getattr_static(SyncPage, "desktop"), marker)
            self.assertNotIn("desktop", vars(AsyncPage))
        finally:
            delattr(SyncPage, "desktop")

    def test_facade_does_not_keep_page_alive(self):
        descriptor = _patch.DesktopAccessor("sync")
        page = FakePage()
        page_ref = weakref.ref(page)
        facade = descriptor.__get__(page, FakePage)
        self.assertIs(descriptor.__get__(page, FakePage), facade)
        del page
        gc.collect()
        self.assertIsNone(page_ref())
        self.assertEqual(len(descriptor._facades), 0)


if __name__ == "__main__":
    unittest.main()
