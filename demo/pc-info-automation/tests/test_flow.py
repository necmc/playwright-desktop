from time import monotonic

from playwright_desktop import DesktopTimeoutError

from config import (
    ACTIVATION_TIMEOUT_MS,
    DEFAULT_DESKTOP_TIMEOUT_MS,
    POLL_INTERVAL_MS,
    POST_TEST_PAUSE_MS,
    PRE_CONTINUE_PAUSE_MS,
)
from fixtures.demo import DemoTestCase
from models.equipment_information import EquipmentInformation
from pages.desktop.desktop_form_window import DesktopFormWindow
from pages.dialogs.browser_open_dialog import BrowserOpenDialog
from pages.dialogs.clickonce_dialog import ClickOnceDialog
from pages.dialogs.sscreen_dialog import SScreenDialog


class PcInfoFlowTests(DemoTestCase):
    def test_confirmed_data_appears_in_web_and_pdf(self):
        """CP-PI-01 · Confirmar datos y generar el PDF."""
        self.info_page.open()
        self.assertFalse(self.pdf_page.is_visible())

        expected = EquipmentInformation(
            date="2026-11-19", computer_name="EQUIPO-CLICKONCE", alias="Integración Ñ"
        )

        app = self._open_desktop_form()
        desktop_form = DesktopFormWindow(app)
        desktop_form.fill(expected)
        desktop_form.confirm()

        self.info_page.wait_for_data()
        self.assertFalse(self.pdf_page.is_visible())
        self.assertEqual(
            self.info_page.read_status(),
            "Información recibida. Revisa los datos y pulsa Continuar para ver el PDF.",
        )
        self.assertEqual(self.info_page.read_data(), expected)

        self.page.wait_for_timeout(PRE_CONTINUE_PAUSE_MS)
        pdf_page = self.info_page.continue_to_pdf()
        self.assertTrue(pdf_page.is_visible())
        pdf_text = pdf_page.read_text()
        for field, value in expected.as_fields().items():
            with self.subTest(pdf_field=field):
                self.assertIn(value, pdf_text)
        self.page.wait_for_timeout(POST_TEST_PAUSE_MS)

    def _open_desktop_form(self):
        sscreen_visible = False
        try:
            with self.page.expect_app(
                title="Información del equipo", timeout=ACTIVATION_TIMEOUT_MS
            ) as opened:
                dcontext = opened.context
                dcontext.set_default_timeout(DEFAULT_DESKTOP_TIMEOUT_MS)
                open_dialog = BrowserOpenDialog(dcontext, self.page)
                clickonce_dialog = ClickOnceDialog(
                    dcontext,
                    application_name="Información del equipo",
                    base_url=self.info_page.base_url,
                )
                sscreen_dialog = SScreenDialog(dcontext, self.page)

                self.info_page.click_obtain()
                open_dialog.click_open()
                sscreen_visible = self._wait_through_dialogs(
                    dcontext, clickonce_dialog, sscreen_dialog
                )
            return opened.value
        except DesktopTimeoutError:
            if sscreen_visible:
                raise DesktopTimeoutError(
                    "Limitado por SScreen. La ventana Java no se abrió dentro del plazo."
                ) from None
            raise

    @staticmethod
    def _wait_through_dialogs(dcontext, clickonce_dialog, sscreen_dialog) -> bool:
        clickonce_accepted = False
        sscreen_accepted = False
        deadline = monotonic() + ACTIVATION_TIMEOUT_MS / 1000
        while monotonic() < deadline:
            if not clickonce_accepted:
                clickonce_accepted = clickonce_dialog.accept_if_present()
            if not sscreen_accepted:
                sscreen_accepted = sscreen_dialog.accept_if_present()
            if DesktopFormWindow.is_ready(dcontext, timeout=POLL_INTERVAL_MS):
                break
        return sscreen_accepted
