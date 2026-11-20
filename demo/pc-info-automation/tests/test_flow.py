import playwright_desktop

from config import ACTIVATION_TIMEOUT_MS
from fixtures.demo import DemoTestCase
from models.equipment_information import EquipmentInformation
from pages.browser_clickonce import BrowserClickOnce
from pages.clickonce_installer import ClickOnceInstaller
from pages.information_form import InformationForm
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
        with page.desktop.expect_window(
            title=InformationForm.TITLE, timeout=ACTIVATION_TIMEOUT_MS
        ) as opened:
            self.web.request_information()
            installer.confirm_after(browser.click_open)

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
