"""Fixtures unittest: cada recurso registra su limpieza al crearse."""

from contextlib import ExitStack
import unittest

from playwright.sync_api import sync_playwright

from config import DEMO_URL
from pages.information_page import InformationPage
from .browser import open_clickonce_context


class DemoTestCase(unittest.TestCase):
    def setUp(self):
        resources = ExitStack()
        self.addCleanup(resources.close)
        self.playwright = resources.enter_context(sync_playwright())

        self.context = resources.enter_context(open_clickonce_context(self.playwright))

        self.page = self.context.new_page()
        self.web = InformationPage(self.page, DEMO_URL)
