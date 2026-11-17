"""Optional structural typing for the dynamically added desktop namespace."""

from typing import Protocol

from ._sync import Desktop
from ._async import Desktop as AsyncDesktop


class HasDesktop(Protocol):
    @property
    def desktop(self) -> Desktop: ...


class HasAsyncDesktop(Protocol):
    @property
    def desktop(self) -> AsyncDesktop: ...
