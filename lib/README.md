# playwright-desktop

Añade `page.desktop` al `Page` oficial de Playwright Python para interactuar con aplicaciones de escritorio.

## Instalación

```shell
pip install playwright-desktop
```

La versión mínima de Python es 3.10.

## Compatibilidad actual

| SO | Nativo | JVM |
| --- | --- | --- |
| Windows | X | X |
| macOS | | |
| Linux | | |

En Windows x64, la librería selecciona el motor de cada ventana automáticamente. Usa UI Automation para accionar botones, enlaces y opciones de menú en aplicaciones nativas, y Java Access Bridge para Java Swing cuando el bridge está habilitado.

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

Para una ventana que ya está abierta, usa `page.desktop.window(title="Opciones")` y busca sus controles con `get_by_role()`.

## Guardar la ventana

Si necesitas usar la ventana como objeto, recupérala con `.value` al salir de `with`.

```python
with page.desktop.expect_window(title="Información del equipo") as opened:
    page.get_by_role("button", name="Obtener información").click()

window = opened.value
window.get_by_role("button", name="Confirmar").click()
```
