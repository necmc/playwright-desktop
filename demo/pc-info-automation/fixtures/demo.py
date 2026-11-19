"""Fixtures unittest: cada recurso registra su limpieza al crearse."""

from contextlib import ExitStack
import unittest

from playwright.sync_api import sync_playwright

from config import DEMO_URL, published_java_home
from pages.information_page import InformationPage
from .browser import open_clickonce_context


class DemoTestCase(unittest.TestCase):
    def setUp(self):
        resources = ExitStack()
        self.addCleanup(resources.close)
        self.playwright = resources.enter_context(sync_playwright())

        self.context = resources.enter_context(open_clickonce_context(self.playwright))

        self.page = self.context.new_page()
        # El runtime de la demo proporciona la DLL de Java Access Bridge.
        if java_home := published_java_home():
            self.page.desktop.configure(java_home=java_home)
        self.page.desktop.set_default_timeout(15_000)
        self.web = InformationPage(self.page, DEMO_URL)
