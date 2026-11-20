"""Fixtures unittest: cada recurso registra su limpieza al crearse."""

from contextlib import ExitStack
import unittest

from playwright.sync_api import sync_playwright

from config import DEMO_URL
from pages.information_page import InformationPage
from .browser import open_clickonce_context, select_browser


class DemoTestCase(unittest.TestCase):
    use_extension_profile = True

    def setUp(self):
        resources = ExitStack()
        self.addCleanup(resources.close)
        self.playwright = resources.enter_context(sync_playwright())

        self.browser_executable = select_browser()
        self.context = resources.enter_context(
            open_clickonce_context(
                self.playwright, self.browser_executable, use_extension_profile=self.use_extension_profile,
            )
        )

        self.page = self.context.pages[0]
        self.web = InformationPage(self.page, DEMO_URL)
