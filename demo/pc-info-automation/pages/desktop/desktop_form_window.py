"""Page Object del formulario desktop Java."""

from playwright_desktop import App, DesktopContext, DesktopTimeoutError

from models.equipment_information import EquipmentInformation


class DesktopFormWindow:
    def __init__(self, app: App):
        self.app = app
        self.date = app.get_by_role("textbox", name="Fecha")
        self.computer_name = app.get_by_role("textbox", name="Nombre del equipo")
        self.alias = app.get_by_role("textbox", name="Alias")
        self.confirm_button = app.get_by_role("button", name="Confirmar")

    @staticmethod
    def is_ready(dcontext: DesktopContext, *, timeout: int) -> bool:
        button = dcontext.app(title="Información del equipo", only_new=True).get_by_role(
            "button", name="Confirmar"
        )
        try:
            button.wait_for(timeout=timeout)
            return True
        except DesktopTimeoutError:
            return False

    def fill(self, information: EquipmentInformation) -> None:
        self.date.fill(information.date)
        self.computer_name.fill(information.computer_name)
        self.alias.fill(information.alias)

    def confirm(self) -> None:
        self.confirm_button.click()
        self.app.wait_for_closed()
