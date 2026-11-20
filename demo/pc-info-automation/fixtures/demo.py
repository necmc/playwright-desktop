"""Fixtures unittest: cada recurso registra su limpieza al crearse."""

from contextlib import ExitStack
import unittest

from playwright.sync_api import sync_playwright

from config import DEMO_URL
from pages.information_page import InformationPage
from .browser import open_browser_context


class DemoTestCase(unittest.TestCase):
    def setUp(self):
        resources = ExitStack()
        self.addCleanup(resources.close)
        playwright = resources.enter_context(sync_playwright())
        context = resources.enter_context(open_browser_context(playwright))

        self.page = context.pages[0]
        self.web = InformationPage(self.page, DEMO_URL)
