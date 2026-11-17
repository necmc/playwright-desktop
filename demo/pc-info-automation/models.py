"""Datos que comparten el formulario Java, la web y el PDF."""

from dataclasses import dataclass


@dataclass(frozen=True)
class EquipmentInformation:
    date: str
    computer_name: str
    alias: str

    def as_payload(self) -> dict[str, str]:
        return {
            "date": self.date,
            "computerName": self.computer_name,
            "alias": self.alias,
        }
