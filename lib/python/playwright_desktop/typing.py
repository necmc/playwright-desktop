"""Optional structural typing for the dynamically added Page.expect_app bridge."""

from typing import Callable, Protocol

from ._async import App as AsyncApp, _AsyncExpectation
from ._sync import App, _Expectation


class HasExpectApp(Protocol):
    def expect_app(
        self,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[App], bool] | None = None,
        timeout: float | None = None,
    ) -> _Expectation[App]: ...


class HasAsyncExpectApp(Protocol):
    def expect_app(
        self,
        *,
        title: str | None = None,
        pid: int | None = None,
        title_prefix: str | None = None,
        has_text: str | None = None,
        predicate: Callable[[AsyncApp], bool] | None = None,
        timeout: float | None = None,
    ) -> _AsyncExpectation[AsyncApp]: ...
