import playwright_desktop

from config import ACTIVATION_TIMEOUT_MS
from fixtures.demo import DemoTestCase
from pages.dialogs.browser_open_dialog import BrowserOpenDialog


class ClickOnceTests(DemoTestCase):
    def test_open_from_browser(self):
        """CP-PI-02 · Pulsar Abrir en la interfaz del navegador."""
        self.info_page.open()
        with self.page.expect_app(timeout=ACTIVATION_TIMEOUT_MS) as opened:
            open_dialog = BrowserOpenDialog(opened.context, self.page)
            self.info_page.click_obtain()
            open_dialog.click_open()
        app = opened.value
        app.close()
