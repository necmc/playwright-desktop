import struct
import sys
from pathlib import Path
import tempfile
import unittest

from playwright_desktop._models import (
    InvokeParams,
    OP_BRING_TO_FRONT,
    OP_CLICK,
    OP_CLOSE_WINDOW,
    OP_PREPARE,
    ROLE_BUTTON,
)

BRIDGE_RELATIVE_PATH = "bin/windowsaccessbridge-64.dll"
STALE_WINDOW_ID = 99999
REQUEST_TIMEOUT_MS = 1000
SHORT_TIMEOUT_MS = 500


@unittest.skipUnless(sys.platform == "win32" and struct.calcsize("P") == 8, "Native bindings require Windows x64.")
class NativeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from playwright_desktop import _native
        cls.native = _native
        cls.engine = _native.Engine()
        cls.directory = tempfile.TemporaryDirectory(prefix="playwright-desktop-native-")
        cls.home = Path(cls.directory.name)
        (cls.home / "bin").mkdir()
        image = bytearray(256)
        image[:2] = b"MZ"
        image[0x3c:0x40] = (128).to_bytes(4, "little")
        image[128:134] = b"PE\0\0\x64\x86"
        (cls.home / BRIDGE_RELATIVE_PATH).write_bytes(image)

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def test_native_session_requires_a_runtime_directory(self):
        with self.assertRaises(self.native.NativeError) as error:
            self.engine.open_session("")
        self.assertEqual(error.exception.args[0], "CONFIGURATION")
        self.assertIn("java_home", error.exception.args[1])

    def test_missing_bridge_reports_the_expected_dll_path(self):
        with self.assertRaises(self.native.NativeError) as error:
            session = self.engine.open_session(str(self.home / "missing"))
            try:
                self.engine.invoke(
                    self.engine.new_request(session, REQUEST_TIMEOUT_MS),
                    InvokeParams(operation=OP_PREPARE, title="test"),
                )
            finally:
                self.engine.close_session(session)
        self.assertEqual(error.exception.args[0], "BRIDGE")
        self.assertIn("windowsaccessbridge-64.dll", error.exception.args[1])

    def test_incorrect_architecture_has_a_typed_diagnostic(self):
        other = self.home / "x86/bin"
        other.mkdir(parents=True, exist_ok=True)
        image = bytearray((self.home / BRIDGE_RELATIVE_PATH).read_bytes())
        image[132:134] = b"\x4c\x01"  # IMAGE_FILE_MACHINE_I386
        (other / "windowsaccessbridge-64.dll").write_bytes(image)
        with self.assertRaises(self.native.NativeError) as error:
            session = self.engine.open_session(str(other.parent))
            try:
                self.engine.invoke(
                    self.engine.new_request(session, REQUEST_TIMEOUT_MS),
                    InvokeParams(operation=OP_PREPARE, title="test"),
                )
            finally:
                self.engine.close_session(session)
        self.assertIn("x64", error.exception.args[1])

    def test_cancellation_before_enqueue_never_loads_the_bridge(self):
        # The fixture is intentionally not loadable: executing it would fail loader validation.
        session = self.engine.open_session(str(self.home))
        request = self.engine.new_request(session, REQUEST_TIMEOUT_MS)
        request.cancel()
        with self.assertRaises(self.native.NativeError) as error:
            self.engine.invoke(
                request,
                InvokeParams(operation=OP_CLICK, title="test", role=ROLE_BUTTON, name="test"),
            )
        self.assertEqual(error.exception.args[0], "CANCELLED")
        self.engine.close_session(session)

    def test_closed_session_cannot_accept_more_requests(self):
        session = self.engine.open_session(str(self.home))
        self.engine.close_session(session)
        with self.assertRaises(self.native.NativeError) as error:
            self.engine.new_request(session, 100)
        self.assertEqual(error.exception.args[0], "CLOSED")

    def test_unknown_operation_is_reported_as_configuration_error(self):
        session = self.engine.open_session(str(self.home))
        self.addCleanup(self.engine.close_session, session)
        request = self.engine.new_request(session, REQUEST_TIMEOUT_MS)
        with self.assertRaises(self.native.NativeError) as error:
            self.engine.invoke(request, InvokeParams(operation="unsupported"))
        self.assertEqual(error.exception.args[0], "CONFIGURATION")
        self.assertIn("Unsupported native operation", error.exception.args[1])

    def test_bring_to_front_and_close_window_validate_handles_and_arguments(self):
        session = self.engine.open_session(str(self.home))
        self.addCleanup(self.engine.close_session, session)
        for operation in (OP_BRING_TO_FRONT, OP_CLOSE_WINDOW):
            with self.subTest(operation=operation, case="stale"):
                request = self.engine.new_request(session, SHORT_TIMEOUT_MS)
                with self.assertRaises(self.native.NativeError) as error:
                    self.engine.invoke(request, InvokeParams(operation=operation, window_id=STALE_WINDOW_ID))
                self.assertEqual(error.exception.args[0], "STALE")
            with self.subTest(operation=operation, case="missing_query"):
                request = self.engine.new_request(session, SHORT_TIMEOUT_MS)
                with self.assertRaises(self.native.NativeError) as error:
                    self.engine.invoke(request, InvokeParams(operation=operation))
                self.assertEqual(error.exception.args[0], "CONFIGURATION")

    def test_os_language_returns_non_empty_bcp47_tag(self):
        tag = self.native.os_language()
        self.assertIsInstance(tag, str)
        self.assertTrue(tag)
        self.assertNotIn("_", tag)


if __name__ == "__main__":
    unittest.main()
