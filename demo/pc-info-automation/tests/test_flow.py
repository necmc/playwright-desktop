from time import monotonic

from playwright_desktop import DesktopTimeoutError

from config import ACTIVATION_TIMEOUT_MS
from fixtures.demo import DemoTestCase
from models.equipment_information import EquipmentInformation
from pages.browser_clickonce import BrowserClickOnce
from pages.clickonce_installer import ClickOnceInstaller
from pages.information_form import InformationForm
from pages.sscreen_warning import SScreenWarning
from utils.pdf import extract_pdf_text


class PcInfoFlowTests(DemoTestCase):
    def test_confirmed_data_appears_in_web_and_pdf(self):
        """CP-PI-01 · Confirmar datos y generar el PDF."""
        self.web.open()
        page = self.page
        page.desktop.set_default_timeout(15_000)

        expected = EquipmentInformation(
            date="2026-11-19", computer_name="EQUIPO-CLICKONCE", alias="Integración Ñ"
        )

        browser = BrowserClickOnce(page)
        installer = ClickOnceInstaller(
            page, application_name=InformationForm.TITLE, base_url=self.web.base_url,
        )
        sscreen = SScreenWarning(page)
        form_ready = page.desktop.window(title=InformationForm.TITLE).get_by_role("button", name="Confirmar")
        sscreen_visible = False
        try:
            with page.desktop.expect_window(
                title=InformationForm.TITLE, timeout=ACTIVATION_TIMEOUT_MS
            ) as opened:
                self.web.request_information()
                browser.click_open()
                clickonce_confirmed = False
                deadline = monotonic() + ACTIVATION_TIMEOUT_MS / 1000
                while monotonic() < deadline:
                    if not clickonce_confirmed:
                        clickonce_confirmed = installer.confirm_if_present()
                    try:
                        form_ready.wait_for(timeout=400)
                        break
                    except DesktopTimeoutError:
                        pass
                    if not sscreen_visible:
                        sscreen_visible = sscreen.confirm_if_present()
        except DesktopTimeoutError:
            if sscreen_visible:
                raise DesktopTimeoutError(
                    "Limitado por SScreen. La ventana Java no se abrió dentro del plazo."
                ) from None
            raise

        form = InformationForm(opened.value)
        form.fill(expected)
        form.confirm()

        self.web.wait_for_result()
        self.assertEqual(self.web.result_message(), "Información recibida. El PDF está listo.")
        self.assertEqual(self.web.read_information(), expected)
        pdf_text = extract_pdf_text(self.web.download_pdf())
        for field, value in expected.as_payload().items():
            with self.subTest(pdf_field=field):
                self.assertIn(value, pdf_text)
        page.wait_for_timeout(5_000)
