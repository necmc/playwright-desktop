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


class FakeContext:
    def __init__(self):
        self.callbacks = {}
        self.closed = False

    def on(self, event, callback):
        self.callbacks[event] = callback

    def is_closed(self):
        return self.closed

    def close(self):
        self.closed = True
        callback = self.callbacks.get("close")
        if callback is not None:
            callback(self)


class FakePage:
    def __init__(self, context=None):
        self.context = FakeContext() if context is None else context
        self.callbacks = {}
        self.closed = False

    def on(self, event, callback):
        self.callbacks[event] = callback

    def is_closed(self):
        return self.closed


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
            self.assertIn("expect_app", vars(cls))
            self.assertNotIn("desktop", vars(cls))
            for name, method in methods.items():
                self.assertIs(getattr(cls, name), method)

    def test_install_and_reload_are_idempotent_and_native_import_is_lazy(self):
        desktop.install()
        descriptor = inspect.getattr_static(SyncPage, "expect_app")
        native_loaded = "playwright_desktop._native" in sys.modules
        desktop.install()
        importlib.reload(desktop)
        self.assertIs(inspect.getattr_static(SyncPage, "expect_app"), descriptor)
        self.assertEqual("playwright_desktop._native" in sys.modules, native_loaded)
        subprocess.run([sys.executable, "-c", "import sys; import playwright_desktop; assert 'playwright_desktop._native' not in sys.modules"], check=True)

    def test_collision_is_detected_before_either_class_is_patched(self):
        marker = object()
        setattr(AsyncPage, "expect_app", marker)
        try:
            with self.assertRaises(PatchConflictError):
                desktop.install()
            self.assertNotIn("expect_app", vars(SyncPage))
            self.assertIs(inspect.getattr_static(AsyncPage, "expect_app"), marker)
        finally:
            delattr(AsyncPage, "expect_app")

    def test_failed_second_assignment_rolls_back_the_first(self):
        class Reject(type):
            def __setattr__(cls, name, value):
                if name == "expect_app":
                    raise RuntimeError("injected assignment failure")
                super().__setattr__(name, value)

        class Second(metaclass=Reject):
            pass

        with patch.object(_patch, "_page_types", return_value=(FakePage, Second)):
            with self.assertRaises(RuntimeError):
                desktop.install()
            self.assertNotIn("expect_app", vars(FakePage))

    def test_uninstall_preserves_a_replacement_owned_by_someone_else(self):
        desktop.install()
        marker = object()
        setattr(SyncPage, "expect_app", marker)
        try:
            desktop.uninstall()
            self.assertIs(inspect.getattr_static(SyncPage, "expect_app"), marker)
            self.assertNotIn("expect_app", vars(AsyncPage))
        finally:
            delattr(SyncPage, "expect_app")

    def test_descriptor_does_not_keep_page_alive(self):
        descriptor = _patch.ExpectAppMethod("sync")
        page = FakePage()
        page_ref = weakref.ref(page)
        bound = descriptor.__get__(page, FakePage)
        self.assertTrue(callable(bound))
        del bound, page
        gc.collect()
        self.assertIsNone(page_ref())

    def test_session_is_shared_by_browser_context_and_survives_page_close(self):
        from playwright_desktop import _runtime

        context = FakeContext()
        page1 = FakePage(context=context)
        page2 = FakePage(context=context)

        session1 = _runtime.session_for_context(page1.context)
        session2 = _runtime.session_for_context(page2.context)
        self.assertIs(session1, session2)
        self.assertIs(session1.browser_context, context)

        # Cerrar page1 no cierra el Session ligado al BrowserContext.
        page1.closed = True
        self.assertFalse(session1.is_closed())
        self.assertFalse(context.is_closed())

        # Cerrar el Session (p. ej. dcontext.close()) no cierra el BrowserContext, y un nuevo expect_app crea una sesión limpia.
        session1.close_nowait()
        self.assertTrue(session1.is_closed())
        self.assertFalse(context.is_closed())
        session3 = _runtime.session_for_context(page2.context)
        self.assertIsNot(session3, session1)

        # Al cerrarse el BrowserContext padre se cierra automáticamente la sesión desktop.
        context.close()
        self.assertTrue(session3.is_closed())

    def test_session_releases_when_browser_context_is_garbage_collected(self):
        from playwright_desktop import _runtime

        context = FakeContext()
        context_ref = weakref.ref(context)
        session = _runtime.session_for_context(context)
        self.assertFalse(session.is_closed())
        del context
        gc.collect()
        self.assertIsNone(context_ref())
        self.assertTrue(session.is_closed())

    def test_module_configure_applies_defaults_to_new_sessions(self):
        from playwright_desktop import _runtime
        from playwright_desktop._models import WindowQuery

        self.addCleanup(_runtime.reset_defaults)
        desktop.configure(java_home="C:/custom-jdk", timeout=4500)
        session = _runtime.session_for_context(FakeContext())
        self.addCleanup(session.close_nowait)
        self.assertTrue(session._java_home.endswith("custom-jdk"))
        self.assertEqual(session.default_timeout, 4500)

        open_capture = WindowQuery.capture()
        self.assertTrue(open_capture.allow_empty)
        with self.assertRaises(ValueError):
            WindowQuery()


if __name__ == "__main__":
    unittest.main()
