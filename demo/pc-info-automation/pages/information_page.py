from urllib.parse import urljoin

from playwright.sync_api import Page

from models.equipment_information import EquipmentInformation


class InformationPage:
    def __init__(self, page: Page, base_url: str):
        self.page = page
        self.base_url = base_url
        self.date = page.get_by_role("textbox", name="Fecha", exact=True)
        self.computer_name = page.get_by_role("textbox", name="Nombre del equipo", exact=True)
        self.alias = page.get_by_role("textbox", name="Alias", exact=True)
        self.obtain_button = page.get_by_role("button", name="Obtener información", exact=True)
        self.launch_link = page.get_by_role("link", name="Abrir aplicación", exact=True)
        self.status = page.get_by_role("status")
        self.pdf_viewer = page.get_by_title("Vista previa de la información confirmada", exact=True)

    def open(self) -> None:
        self.page.goto(self.base_url)

    def request_information(self) -> None:
        self.obtain_button.click()
        self.launch_link.click()

    def wait_for_result(self) -> None:
        self.pdf_viewer.wait_for(state="visible")

    def result_message(self) -> str:
        return self.status.inner_text()

    def read_information(self) -> EquipmentInformation:
        return EquipmentInformation(
            date=self.date.input_value(),
            computer_name=self.computer_name.input_value(),
            alias=self.alias.input_value(),
        )

    def download_pdf(self) -> bytes:
        address = self.pdf_viewer.get_attribute("src")
        if not address:
            raise RuntimeError("El visor no tiene una URL de PDF.")
        response = self.page.request.get(urljoin(self.base_url, address))
        if not response.ok:
            raise RuntimeError(f"No se pudo descargar el PDF: HTTP {response.status}.")
        return response.body()
