import asyncio
import gc
import os
from threading import Event, get_ident
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import weakref

from playwright_desktop._async import Desktop
from playwright_desktop._errors import BridgeUnavailableError, DesktopTimeoutError
from playwright_desktop._runtime import Session
from test_patch import FakePage


class Request:
    def __init__(self):
        self.cancelled = False

    def cancel(self):
        self.cancelled = True


class Engine:
    def __init__(self):
        self.entered = Event()
        self.release = Event()
        self.request = None
        self.native_thread = None
        self.closed = []

    def open_auto_session(self, home, *, explicit=False):
        return 1

    def new_request(self, session, timeout):
        self.request = Request()
        return self.request

    def invoke(self, request, operation, **parameters):
        self.native_thread = get_ident()
        self.entered.set()
        if not self.release.wait(2):
            raise RuntimeError("The test did not release its native call.")
        return SimpleNamespace(handle=2, text="Álias \U00010400")

    def close_session(self, session):
        self.closed.append(session)


class AsyncTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.page = FakePage()
        self.session = Session(self.page)
        self.session.configure(java_home="C:/test-java")
        self.desktop = Desktop(self.session)
        self.engine = Engine()
        self.engine_patch = patch("playwright_desktop._runtime._get_engine", return_value=self.engine)
        self.load_engine = self.engine_patch.start()

    async def asyncTearDown(self):
        self.engine.release.set()
        self.session.close_nowait()
        self.engine_patch.stop()

    async def wait_until_set(self, event):
        async def ready():
            while not event.is_set():
                await asyncio.sleep(0.001)
        await asyncio.wait_for(ready(), 1)

    async def test_native_wait_does_not_block_the_loop_or_use_the_page_thread(self):
        owner = get_ident()
        task = asyncio.create_task(self.desktop.window(title="My window").get_by_role("textbox", name="Alias").input_value())
        await self.wait_until_set(self.engine.entered)
        self.assertFalse(task.done())
        self.assertNotEqual(self.engine.native_thread, owner)
        self.engine.release.set()
        self.assertEqual(await task, "Álias \U00010400")

    async def test_cancellation_marks_the_request_even_while_native_code_is_running(self):
        task = asyncio.create_task(self.desktop.window(title="My window").get_by_role("textbox", name="Alias").input_value())
        await self.wait_until_set(self.engine.entered)
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await task
        self.assertTrue(self.engine.request.cancelled)
        self.engine.release.set()

    async def test_page_close_releases_its_native_session(self):
        self.engine.release.set()
        await self.desktop.window(title="My window").get_by_role("textbox", name="Alias").input_value()
        self.page.callbacks["close"]()
        self.assertEqual(self.engine.closed, [1])

    async def test_cold_request_uses_the_engine_returned_by_the_background_load(self):
        self.engine.release.set()
        alias = self.desktop.window(title="My window").get_by_role("textbox", name="Alias")
        self.assertEqual(await alias.input_value(), "Álias \U00010400")
        self.load_engine.assert_called_once_with()

    async def test_slow_engine_start_does_not_block_the_event_loop(self):
        started = Event()
        release = Event()
        self.engine.release.set()

        def load():
            started.set()
            if not release.wait(3):
                raise RuntimeError("The event loop did not release the engine startup.")
            return self.engine

        self.load_engine.side_effect = load
        alias = self.desktop.window(title="My window").get_by_role("textbox", name="Alias")
        task = asyncio.create_task(alias.input_value())
        try:
            await self.wait_until_set(started)
            heartbeat = asyncio.Event()
            asyncio.get_running_loop().call_soon(heartbeat.set)
            await asyncio.wait_for(heartbeat.wait(), 1)
            self.assertFalse(task.done())
        finally:
            release.set()
            await task

    async def test_page_access_and_close_callback_stay_in_the_owner_loop(self):
        owner = (get_ident(), asyncio.get_running_loop())
        observed = []

        def record(action):
            observed.append((action, (get_ident(), asyncio.get_running_loop())))

        class Page(FakePage):
            def on(self, event, callback):
                record("register")
                super().on(event, callback)

            def is_closed(self):
                record("check")
                return False

        page = Page()
        session = Session(page)
        self.addCleanup(session.close_nowait)
        session.configure(java_home="C:/test-java")
        desktop = Desktop(session)
        self.engine.release.set()
        alias = desktop.window(title="My window").get_by_role("textbox", name="Alias")
        await alias.input_value()

        def close_session(session_id):
            record("close")
            self.engine.closed.append(session_id)

        with patch.object(self.engine, "close_session", side_effect=close_session):
            page.callbacks["close"]()

        self.assertEqual({action for action, _ in observed}, {"register", "check", "close"})
        self.assertTrue(all(context == owner for _, context in observed))
        self.assertNotEqual(self.engine.native_thread, owner[0])

    async def test_page_event_collection_and_explicit_close_release_pending_sessions(self):
        for close_by in ("page", "collection", "desktop"):
            with self.subTest(close_by=close_by):
                page = FakePage()
                session = Session(page)
                self.addCleanup(session.close_nowait)
                session.configure(java_home="C:/test-java")
                desktop = Desktop(session)
                engine = Engine()
                self.load_engine.return_value = engine
                alias = desktop.window(title="My window").get_by_role("textbox", name="Alias")
                task = asyncio.create_task(alias.input_value())
                try:
                    await self.wait_until_set(engine.entered)
                    if close_by == "page":
                        page.callbacks["close"]()
                    elif close_by == "collection":
                        page_ref = weakref.ref(page)
                        del page
                        gc.collect()
                        self.assertIsNone(page_ref())
                    else:
                        await desktop.close()
                    self.assertEqual(engine.closed, [1])
                    self.assertFalse(task.done())
                finally:
                    engine.release.set()
                    await task

    async def test_native_action_requires_no_java_home(self):
        page = FakePage()
        session = Session(page)
        desktop = Desktop(session)
        self.addCleanup(session.close_nowait)
        self.engine.release.set()
        with patch.dict(os.environ):
            os.environ.pop("JAVA_HOME", None)
            with patch.object(self.engine, "open_auto_session", return_value=1) as open_session:
                await desktop.window(title="Opciones").get_by_role("link", name="Abrir").click()
        open_session.assert_called_once_with(None, explicit=False)

    async def test_native_timeout_preserves_its_original_cause(self):
        class NativeFailure(Exception):
            pass
        with patch("playwright_desktop._runtime._native_error", NativeFailure):
            with patch.object(self.engine, "invoke", side_effect=NativeFailure("TIMEOUT", "native timeout")):
                with self.assertRaises(DesktopTimeoutError) as captured:
                    await self.desktop.window(title="My window").get_by_role("textbox", name="Alias").input_value()
        self.assertIsInstance(captured.exception.__cause__, NativeFailure)

    async def test_request_creation_errors_use_the_public_error_hierarchy(self):
        class NativeFailure(Exception):
            pass
        with patch("playwright_desktop._runtime._native_error", NativeFailure):
            with patch.object(self.engine, "new_request", side_effect=NativeFailure("BRIDGE", "restart")):
                with self.assertRaises(BridgeUnavailableError) as captured:
                    await self.desktop.window(title="My window").get_by_role("textbox", name="Alias").input_value()
        self.assertIsInstance(captured.exception.__cause__, NativeFailure)

    async def test_desktop_actions_and_event_value_use_the_same_captured_window(self):
        replies = [SimpleNamespace(handle=handle, text="QA") for handle in (1, 2, 2, 2)]
        with patch.object(self.engine, "invoke", side_effect=replies) as invoke:
            async with self.desktop.expect_window(title="My window") as opened:
                pass
            window = await opened.value
            self.assertEqual(await self.desktop.get_by_role("textbox", name="Alias").input_value(), "QA")
            await window.get_by_role("button", name="Confirmar").click()

        self.assertEqual([call.args[1] for call in invoke.call_args_list],
                         ["prepare", "capture", "read_text", "click"])
        self.assertEqual([call.kwargs["window_id"] for call in invoke.call_args_list[2:]], [2, 2])

    async def test_window_has_text_forwards_the_visible_text_filter_async(self):
        self.engine.release.set()
        with patch.object(self.engine, "invoke", return_value=SimpleNamespace(handle=3, text=None)) as invoke:
            await self.desktop.window(has_text="Windows protegió su PC").get_by_role("link", name="Más información").click()
        self.assertEqual(invoke.call_args.kwargs["has_text"], "Windows protegió su PC")


if __name__ == "__main__":
    unittest.main()
