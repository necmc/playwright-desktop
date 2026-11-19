# playwright-desktop

Añade `page.desktop` al `Page` oficial de Playwright Python para interactuar con aplicaciones de escritorio.

## Instalación

```shell
pip install playwright-desktop
```

Requiere Python 3.10–3.14 y Windows x64.

## Sintaxis básica

Sobre un `page` de Playwright, pon el clic que abre la ventana dentro de `with` y usa `page.desktop` al salir.

```python
import playwright_desktop

with page.desktop.expect_window(title="Información del equipo"):
    page.get_by_role("button", name="Obtener información").click()

page.desktop.get_by_role("textbox", name="Alias").fill("QA")
page.desktop.get_by_role("button", name="Confirmar").click()
```

Playwright usa el mismo patrón con `page.expect_popup()` para una ventana del navegador.

```python
with page.expect_popup():
    page.get_by_role("link", name="Abrir reporte").click()
```

Con Playwright asíncrono, usa `async with` y `await` en las acciones.

## Guardar la ventana

Si necesitas usar la ventana como objeto, recupérala con `.value` al salir de `with`.

```python
with page.desktop.expect_window(title="Información del equipo") as opened:
    page.get_by_role("button", name="Obtener información").click()

window = opened.value
window.get_by_role("button", name="Confirmar").click()
```
