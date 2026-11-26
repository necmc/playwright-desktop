"""Page Object del formulario web principal."""

from playwright.sync_api import Page

from models.equipment_information import EquipmentInformation
from pages.web.pdf_preview_page import PdfPreviewPage


class InformationPage:
    def __init__(self, page: Page, base_url: str):
        self.page = page
        self.base_url = base_url
        self.date = page.get_by_role("textbox", name="Fecha", exact=True)
        self.computer_name = page.get_by_role("textbox", name="Nombre del equipo", exact=True)
        self.alias = page.get_by_role("textbox", name="Alias", exact=True)
        self.obtain_button = page.get_by_role("button", name="Obtener información", exact=True)
        self.continue_button = page.get_by_role("button", name="Continuar", exact=True)
        self.status = page.get_by_role("status")

    def open(self) -> None:
        self.page.goto(self.base_url)

    def click_obtain(self) -> None:
        self.obtain_button.click()

    def wait_for_data(self) -> None:
        self.continue_button.wait_for(state="visible")

    def read_status(self) -> str:
        return self.status.inner_text()

    def read_data(self) -> EquipmentInformation:
        return EquipmentInformation(
            date=self.date.input_value(),
            computer_name=self.computer_name.input_value(),
            alias=self.alias.input_value(),
        )

    def continue_to_pdf(self) -> PdfPreviewPage:
        self.continue_button.click()
        pdf_page = PdfPreviewPage(self.page, self.base_url)
        pdf_page.wait_until_visible()
        return pdf_page
