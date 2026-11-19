"""Monotonic time budgets shared by captures and native requests."""

from math import ceil
from time import monotonic

from ._errors import DesktopTimeoutError

DEFAULT_TIMEOUT_MS = 10_000


class Deadline:
    def __init__(self, timeout_ms: float) -> None:
        self._timeout_ms = timeout_ms
        self._started = monotonic()

    def remaining_ms(self, message: str) -> int:
        if self._timeout_ms == 0:
            return 0
        remaining = self._timeout_ms - (monotonic() - self._started) * 1000
        if remaining <= 0:
            raise DesktopTimeoutError(message)
        return ceil(remaining)
