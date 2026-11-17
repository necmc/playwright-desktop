from playwright.sync_api import Page

from models import EquipmentInformation


class InformationForm:
    TITLE = "Información del equipo"

    def __init__(self, page: Page):
        self.window = page.desktop.window(title=self.TITLE)
        self.date = self.window.get_by_role("textbox", name="Fecha")
        self.computer_name = self.window.get_by_role("textbox", name="Nombre del equipo")
        self.alias = self.window.get_by_role("textbox", name="Alias")
        self.confirm_button = self.window.get_by_role("button", name="Confirmar")

    def fill(self, information: EquipmentInformation) -> None:
        self.date.fill(information.date)
        self.computer_name.fill(information.computer_name)
        self.alias.fill(information.alias)

    def confirm(self) -> None:
        self.confirm_button.click()
        self.window.wait_for_closed()
