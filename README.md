# playwright-desktop

Extiende el `Page` oficial de Playwright Python con `page.expect_app()` y un `DesktopContext` ligado al `BrowserContext` para interactuar con aplicaciones de escritorio.

## Instalación

```shell
pip install playwright-desktop
```

Requiere Windows x64 y Python 3.10 como mínimo.

## Uso básico

Sobre un `page` que ya tengas creado con Playwright:

```python
import playwright_desktop

with page.expect_app(title="Mi aplicación") as app_info:
    page.get_by_role("button", name="Abrir aplicación").click()

app = app_info.value
app.get_by_role("textbox", name="Nombre").fill("Ana")
app.get_by_role("button", name="Aceptar").click()
```

Con Playwright asíncrono, usa `async with`, `app = await app_info.value` y `await` para las acciones.

## Estructura

```text
lib/     Librería Python y motor Rust
demo/    Ejemplos de integración y sus pruebas
docs/    Documentación técnica y arquitectura
```

## Más información

- [Demo y automatización](demo/README.md).
- [Documentación técnica](docs/documentacion-tecnica.md).
- [Diseño DesktopContext y App](docs/superpowers/specs/2026-11-20-desktop-context-app-design.md).
