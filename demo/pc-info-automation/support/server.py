import sys
from threading import Thread

from .settings import PROJECT_ROOT, Settings


class DemoServer:
    """Servidor de la demo durante el escenario funcional."""

    def __init__(self, settings: Settings):
        from werkzeug.serving import make_server

        web_path = str(PROJECT_ROOT / "demo/pc-info/web")
        sys.path.insert(0, web_path)
        try:
            from app import create_app
        finally:
            sys.path.remove(web_path)

        self.server = make_server("127.0.0.1", settings.port, create_app(), threaded=True)
        self.thread = Thread(target=self.server.serve_forever, name="demo-test-server", daemon=True)
        try:
            self.thread.start()
        except BaseException:
            self.server.server_close()
            raise

    def close(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)
