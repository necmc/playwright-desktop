"""Operaciones locales del intercambio entre la web y Java."""

from dataclasses import dataclass
from datetime import date
import secrets
from threading import Lock
from time import monotonic

from pdf_report import create_report


class OperationError(Exception):
    def __init__(self, message: str, status_code: int):
        super().__init__(message)
        self.status_code = status_code


@dataclass
class Operation:
    created_at: float
    status: str = "pending"
    data: dict[str, str] | None = None
    pdf: bytes | None = None
    error: str | None = None


def validate_data(values) -> dict[str, str]:
    result = {key: values.get(key, "").strip() for key in ("date", "computerName", "alias")}
    try:
        if date.fromisoformat(result["date"]).isoformat() != result["date"]:
            raise ValueError
    except ValueError:
        raise OperationError("La fecha debe ser válida y tener formato YYYY-MM-DD.", 422) from None
    for key, label in (("computerName", "nombre del equipo"), ("alias", "alias")):
        value = result[key]
        if not 1 <= len(value) <= 100 or not value.isprintable():
            raise OperationError(f"El {label} debe contener entre 1 y 100 caracteres visibles.", 422)
    return result


class OperationStore:
    def __init__(self, lifetime: int = 1200):
        self.lifetime = lifetime
        self._operations: dict[str, Operation] = {}
        self._lock = Lock()

    def _get(self, operation_id: str) -> Operation:
        operation = self._operations.get(operation_id)
        if operation is None:
            raise OperationError("Operación desconocida.", 404)
        if monotonic() - operation.created_at >= self.lifetime:
            raise OperationError("La operación expiró. Inicia otra desde la web.", 410)
        return operation

    def create(self) -> str:
        with self._lock:
            now = monotonic()
            self._operations = {
                key: operation for key, operation in self._operations.items()
                if now - operation.created_at < self.lifetime
            }
            if len(self._operations) >= 100:
                raise OperationError("Hay demasiadas operaciones activas. Intenta más tarde.", 429)
            operation_id = secrets.token_urlsafe(24)
            self._operations[operation_id] = Operation(created_at=now)
            return operation_id

    def snapshot(self, operation_id: str) -> dict:
        with self._lock:
            operation = self._get(operation_id)
            return {"status": operation.status, "data": operation.data, "error": operation.error}

    def confirm(self, operation_id: str, values) -> dict:
        data = validate_data(values)
        with self._lock:
            operation = self._get(operation_id)
            if operation.status == "completed" and operation.data == data:
                return data
            if operation.status != "pending":
                raise OperationError("La operación ya finalizó.", 409)
            pdf = create_report(data)
            operation.pdf = pdf
            operation.data = data
            operation.status = "completed"
            return data

    def finish(self, operation_id: str, status: str, message: str | None = None):
        with self._lock:
            operation = self._get(operation_id)
            if operation.status == "completed":
                return
            if operation.status not in ("pending", status):
                raise OperationError("La operación ya finalizó.", 409)
            operation.status = status
            operation.error = message

    def document(self, operation_id: str) -> bytes:
        with self._lock:
            operation = self._get(operation_id)
            if operation.pdf is None:
                raise OperationError("El PDF todavía no está disponible.", 409)
            return operation.pdf
