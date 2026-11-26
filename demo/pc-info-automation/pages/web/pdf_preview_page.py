"""Page Object de la vista previa del PDF."""

from urllib.parse import urljoin

from playwright.sync_api import Page

from utils.pdf import extract_pdf_text


class PdfPreviewPage:
    def __init__(self, page: Page, base_url: str):
        self.page = page
        self.base_url = base_url
        self.section = page.get_by_role("region", name="Vista previa del PDF", exact=True)
        self.viewer = page.get_by_title("Vista previa de la información confirmada", exact=True)
        self.back_button = page.get_by_role("button", name="Volver", exact=True)

    def is_visible(self) -> bool:
        return self.section.is_visible()

    def wait_until_visible(self) -> None:
        self.viewer.wait_for(state="visible")

    def read_text(self) -> str:
        return extract_pdf_text(self._download())

    def go_back(self) -> None:
        self.back_button.click()

    def _download(self) -> bytes:
        address = self.viewer.get_attribute("src")
        if not address:
            raise RuntimeError("El visor no tiene una URL de PDF.")
        response = self.page.request.get(urljoin(self.base_url, address))
        if not response.ok:
            raise RuntimeError(f"No se pudo descargar el PDF: HTTP {response.status}.")
        return response.body()
