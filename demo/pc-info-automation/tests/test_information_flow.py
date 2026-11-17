import playwright_desktop

from models import EquipmentInformation
from pages.information_form import InformationForm
from support.fixtures import DemoTestCase
from support.pdf import extract_pdf_text


class InformationFlowTests(DemoTestCase):
    def test_confirmed_information_appears_in_the_web_and_pdf(self):
        page = self.page
        page.desktop.configure(java_home=self.settings.java_home)
        page.desktop.set_default_timeout(15_000)

        expected = EquipmentInformation(
            date="2026-11-19", computer_name="EQUIPO-CLICKONCE", alias="Integración Ñ"
        )
        self.web.open()

        with page.desktop.expect_window(
            title=InformationForm.TITLE, timeout=self.settings.activation_timeout
        ):
            self.web.request_information()

        form = InformationForm(page)
        form.fill(expected)
        form.confirm()

        self.web.wait_for_result()
        self.assertEqual(self.web.read_information(), expected)
        pdf_text = extract_pdf_text(self.web.download_pdf())
        for field, value in expected.as_payload().items():
            with self.subTest(pdf_field=field):
                self.assertIn(value, pdf_text)
