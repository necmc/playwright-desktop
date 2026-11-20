import playwright_desktop

from config import ACTIVATION_TIMEOUT_MS, published_java_home
from fixtures.demo import DemoTestCase
from models.equipment_information import EquipmentInformation
from pages.information_form import InformationForm
from utils.pdf import extract_pdf_text


class PcInfoFlowTests(DemoTestCase):
    def test_confirmed_data_appears_in_web_and_pdf(self):
        """CP-PI-01 · Confirmar datos y generar el PDF."""
        self.web.open()
        page = self.page
        # La publicación de la demo indica dónde localizar Java Access Bridge.
        if java_home := published_java_home():
            page.desktop.configure(java_home=java_home)
        page.desktop.set_default_timeout(15_000)

        expected = EquipmentInformation(
            date="2026-11-19", computer_name="EQUIPO-CLICKONCE", alias="Integración Ñ"
        )

        with page.desktop.expect_window(
            title=InformationForm.TITLE, timeout=ACTIVATION_TIMEOUT_MS
        ) as opened:
            self.web.request_information()

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
