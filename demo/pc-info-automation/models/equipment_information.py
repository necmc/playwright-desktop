"""Objeto de transferencia de datos del formulario, la web y el PDF."""

from dataclasses import dataclass

FIELD_DATE = "date"
FIELD_COMPUTER_NAME = "computerName"
FIELD_ALIAS = "alias"


@dataclass(frozen=True)
class EquipmentInformation:
    date: str
    computer_name: str
    alias: str

    def as_fields(self) -> dict[str, str]:
        return {
            FIELD_DATE: self.date,
            FIELD_COMPUTER_NAME: self.computer_name,
            FIELD_ALIAS: self.alias,
        }
