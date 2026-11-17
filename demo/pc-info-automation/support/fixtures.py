"""Fixtures unittest: cada recurso registra su limpieza al crearse."""

from contextlib import ExitStack
import unittest

from playwright.sync_api import sync_playwright

from pages.information_page import InformationPage
from .browser import open_clickonce_context
from .server import DemoServer
from .settings import Settings


class DemoTestCase(unittest.TestCase):
    settings: Settings | None = None

    def setUp(self):
        self.settings = self.settings or Settings.load()
        self.settings.validate()
        resources = ExitStack()
        self.addCleanup(resources.close)
        self.demo = DemoServer(self.settings)
        resources.callback(self.demo.close)
        self.playwright = resources.enter_context(sync_playwright())

        self.context = resources.enter_context(open_clickonce_context(self.playwright))

        self.page = self.context.new_page()
        self.web = InformationPage(self.page, self.settings.base_url)
