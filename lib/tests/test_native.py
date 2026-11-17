import struct
import sys
from pathlib import Path
import tempfile
import unittest


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
        (cls.home / "bin/windowsaccessbridge-64.dll").write_bytes(image)

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def test_missing_bridge_and_incorrect_architecture_have_typed_diagnostics(self):
        with self.assertRaises(self.native.NativeError) as error:
            session = self.engine.open_session(str(self.home / "missing"))
            try:
                self.engine.invoke(self.engine.new_request(session, 1000), "prepare", title="test")
            finally:
                self.engine.close_session(session)
        self.assertEqual(error.exception.args[0], "BRIDGE")
        other = self.home / "x86/bin"
        other.mkdir(parents=True, exist_ok=True)
        (other / "windowsaccessbridge-64.dll").write_bytes(b"not a PE file")
        with self.assertRaises(self.native.NativeError) as error:
            session = self.engine.open_session(str(other.parent))
            try:
                self.engine.invoke(self.engine.new_request(session, 1000), "prepare", title="test")
            finally:
                self.engine.close_session(session)
        self.assertIn("x64", error.exception.args[1])

    def test_cancellation_before_enqueue_never_loads_the_bridge(self):
        # The fixture is intentionally not loadable: executing it would fail loader validation.
        session = self.engine.open_session(str(self.home))
        request = self.engine.new_request(session, 1000)
        request.cancel()
        with self.assertRaises(self.native.NativeError) as error:
            self.engine.invoke(request, "click", title="test", role="button", name="test")
        self.assertEqual(error.exception.args[0], "CANCELLED")
        self.engine.close_session(session)

    def test_closed_session_cannot_accept_more_requests(self):
        session = self.engine.open_session(str(self.home))
        self.engine.close_session(session)
        with self.assertRaises(self.native.NativeError) as error:
            self.engine.new_request(session, 100)
        self.assertEqual(error.exception.args[0], "CLOSED")


if __name__ == "__main__":
    unittest.main()
