"""Operaciones locales del intercambio entre la web y Java."""

from dataclasses import dataclass
from datetime import date
from http import HTTPStatus
import secrets
from threading import Lock
from time import monotonic

from pdf_report import create_report

DEFAULT_OPERATION_LIFETIME_S = 1200
MAX_ACTIVE_OPERATIONS = 100
OPERATION_TOKEN_BYTES = 24
MIN_TEXT_LENGTH = 1
MAX_TEXT_LENGTH = 100

FIELD_DATE = "date"
FIELD_COMPUTER_NAME = "computerName"
FIELD_ALIAS = "alias"
ALL_FIELDS = (FIELD_DATE, FIELD_COMPUTER_NAME, FIELD_ALIAS)
TEXT_FIELDS = ((FIELD_COMPUTER_NAME, "nombre del equipo"), (FIELD_ALIAS, "alias"))

STATUS_PENDING = "pending"
STATUS_COMPLETED = "completed"
STATUS_CANCELLED = "cancelled"
STATUS_FAILED = "failed"


class OperationError(Exception):
    def __init__(self, message: str, status_code: int):
        super().__init__(message)
        self.status_code = int(status_code)


@dataclass
class Operation:
    created_at: float
    status: str = STATUS_PENDING
    data: dict[str, str] | None = None
    pdf: bytes | None = None
    error: str | None = None


def validate_data(values) -> dict[str, str]:
    result = {key: values.get(key, "").strip() for key in ALL_FIELDS}
    try:
        if date.fromisoformat(result[FIELD_DATE]).isoformat() != result[FIELD_DATE]:
            raise ValueError
    except ValueError:
        raise OperationError(
            "La fecha debe ser válida y tener formato YYYY-MM-DD.",
            HTTPStatus.UNPROCESSABLE_ENTITY,
        ) from None
    for key, label in TEXT_FIELDS:
        value = result[key]
        if not MIN_TEXT_LENGTH <= len(value) <= MAX_TEXT_LENGTH or not value.isprintable():
            raise OperationError(
                f"El {label} debe contener entre {MIN_TEXT_LENGTH} y {MAX_TEXT_LENGTH} caracteres visibles.",
                HTTPStatus.UNPROCESSABLE_ENTITY,
            )
    return result


class OperationStore:
    def __init__(self, lifetime: int = DEFAULT_OPERATION_LIFETIME_S):
        self.lifetime = lifetime
        self._operations: dict[str, Operation] = {}
        self._lock = Lock()

    def _get(self, operation_id: str) -> Operation:
        operation = self._operations.get(operation_id)
        if operation is None:
            raise OperationError("Operación desconocida.", HTTPStatus.NOT_FOUND)
        if monotonic() - operation.created_at >= self.lifetime:
            raise OperationError("La operación expiró. Inicia otra desde la web.", HTTPStatus.GONE)
        return operation

    def create(self) -> str:
        with self._lock:
            now = monotonic()
            self._operations = {
                key: operation for key, operation in self._operations.items()
                if now - operation.created_at < self.lifetime
            }
            if len(self._operations) >= MAX_ACTIVE_OPERATIONS:
                raise OperationError(
                    "Hay demasiadas operaciones activas. Intenta más tarde.",
                    HTTPStatus.TOO_MANY_REQUESTS,
                )
            operation_id = secrets.token_urlsafe(OPERATION_TOKEN_BYTES)
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
            if operation.status == STATUS_COMPLETED and operation.data == data:
                return data
            if operation.status != STATUS_PENDING:
                raise OperationError("La operación ya finalizó.", HTTPStatus.CONFLICT)
            pdf = create_report(data)
            operation.pdf = pdf
            operation.data = data
            operation.status = STATUS_COMPLETED
            return data

    def finish(self, operation_id: str, status: str, message: str | None = None):
        with self._lock:
            operation = self._get(operation_id)
            if operation.status == STATUS_COMPLETED:
                return
            if operation.status not in (STATUS_PENDING, status):
                raise OperationError("La operación ya finalizó.", HTTPStatus.CONFLICT)
            operation.status = status
            operation.error = message

    def document(self, operation_id: str) -> bytes:
        with self._lock:
            operation = self._get(operation_id)
            if operation.pdf is None:
                raise OperationError("El PDF todavía no está disponible.", HTTPStatus.CONFLICT)
            return operation.pdf
