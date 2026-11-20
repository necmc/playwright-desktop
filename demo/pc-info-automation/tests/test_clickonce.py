import playwright_desktop

from fixtures.demo import DemoTestCase
from pages.browser_clickonce import BrowserClickOnce


class ClickOnceTests(DemoTestCase):
    use_extension_profile = False

    def test_open_from_browser(self):
        """CP-PI-02 · Pulsar Abrir en la interfaz del navegador."""
        self.web.open()
        self.web.request_information()
        BrowserClickOnce(self.page, self.browser_executable).click_open()
