import playwright_desktop

from config import ACTIVATION_TIMEOUT_MS
from fixtures.demo import DemoTestCase
from pages.browser_clickonce import BrowserClickOnce


class ClickOnceTests(DemoTestCase):
    def test_open_from_browser(self):
        """CP-PI-02 · Pulsar Abrir en la interfaz del navegador."""
        self.web.open()
        with self.page.expect_app(timeout=ACTIVATION_TIMEOUT_MS) as opened:
            self.web.request_information()
            BrowserClickOnce(opened.context, self.page).click_open()
        app = opened.value
        app.close()
