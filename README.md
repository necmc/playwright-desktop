# playwright-desktop

Extiende el `Page` oficial de Playwright Python con `page.desktop` para interactuar con aplicaciones de escritorio.

## Instalación

```shell
pip install playwright-desktop
```

Requiere Windows x64 y Python 3.10 como mínimo.

## Uso básico

Sobre un `page` que ya tengas creado con Playwright:

```python
import playwright_desktop

with page.desktop.expect_window(title="Mi aplicación"):
    page.get_by_role("button", name="Abrir aplicación").click()

page.desktop.get_by_role("textbox", name="Nombre").fill("Ana")
page.desktop.get_by_role("button", name="Aceptar").click()
```

Con Playwright asíncrono, usa `async with` para capturar la ventana y `await` para las acciones.

## Estructura

```text
lib/     Librería Python y motor Rust
demo/    Ejemplos de integración y sus pruebas
docs/    Documentación técnica y arquitectura
```

## Más información

- [Demo y automatización](demo/README.md).
- [API y arquitectura](docs/superpowers/specs/2026-11-17-libreria-python-rust-design.md).
