# playwright-desktop

Añade `page.desktop` al `Page` oficial de Playwright Python para interactuar con aplicaciones de escritorio.

## Instalación

```shell
pip install playwright-desktop
```

Requiere Python 3.10–3.14 y Windows x64.

## Sintaxis básica

Importa la librería y utiliza el `page` que ya tienes creado con Playwright:

```python
import playwright_desktop

window = page.desktop.window(title="Mi aplicación")
window.get_by_role("textbox", name="Nombre").fill("Ana")
window.get_by_role("button", name="Confirmar").click()
```

Con la API asíncrona de Playwright, utiliza `await` en las acciones.
