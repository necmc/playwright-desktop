# playwright-desktop

Extiende el `Page` oficial de Playwright Python con `page.desktop` para interactuar con aplicaciones de escritorio.

## Instalación

```shell
pip install playwright-desktop
```

Requiere Python 3.10–3.14 y Windows x64.

## Uso básico

Sobre un `page` que ya tengas creado con Playwright:

```python
import playwright_desktop

window = page.desktop.window(title="Mi aplicación")
window.get_by_role("textbox", name="Nombre").fill("Valor")
window.get_by_role("button", name="Aceptar").click()
```

Con la API asíncrona de Playwright, utiliza `await` en acciones como `fill()` y `click()`. Los imports y las acciones web habituales se conservan.

## Estructura

```text
lib/     Librería Python y motor Rust
demo/    Ejemplos de integración y sus pruebas
docs/    Documentación técnica y arquitectura
```

## Más información

- [Demo y automatización](demo/README.md).
- [API y arquitectura](docs/superpowers/specs/2026-11-17-libreria-python-rust-design.md).
