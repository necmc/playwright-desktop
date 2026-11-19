import asyncio
from threading import Event, get_ident
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from playwright_desktop._async import Desktop
from playwright_desktop._errors import DesktopTimeoutError, BridgeUnavailableError
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

    def open_session(self, home):
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
        self.engine_patch.start()

    async def asyncTearDown(self):
        self.engine.release.set()
        self.session.close_nowait()
        self.engine_patch.stop()

    async def wait_until_entered(self):
        async def ready():
            while not self.engine.entered.is_set():
                await asyncio.sleep(0.001)
        await asyncio.wait_for(ready(), 1)

    async def test_native_wait_does_not_block_the_loop_or_use_the_page_thread(self):
        owner = get_ident()
        task = asyncio.create_task(self.desktop.window(title="My window").get_by_role("textbox", name="Alias").input_value())
        await self.wait_until_entered()
        self.assertFalse(task.done())
        self.assertNotEqual(self.engine.native_thread, owner)
        self.engine.release.set()
        self.assertEqual(await task, "Álias \U00010400")

    async def test_cancellation_marks_the_request_even_while_native_code_is_running(self):
        task = asyncio.create_task(self.desktop.window(title="My window").get_by_role("textbox", name="Alias").input_value())
        await self.wait_until_entered()
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


if __name__ == "__main__":
    unittest.main()
