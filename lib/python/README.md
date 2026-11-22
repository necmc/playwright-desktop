# playwright-desktop

Añade `page.expect_app()` al `Page` oficial de Playwright Python para automatizar aplicaciones de escritorio a través de `DesktopContext`, `App`, `Window` y `Alert`.

## Instalación

```shell
pip install playwright-desktop
```

La versión mínima de Python es 3.10.

## Compatibilidad actual

| SO      | Nativo | JVM |
| ------- | ------ | --- |
| Windows | X      | X   |
| macOS   |        |     |
| Linux   |        |     |

En Windows x64, la librería selecciona el motor de cada ventana automáticamente. Usa UI Automation para accionar botones, enlaces y opciones de menú en aplicaciones nativas, y Java Access Bridge para Java Swing cuando el bridge está habilitado.

## Sintaxis básica

Sobre un `page` de Playwright, coloca dentro de `with` la acción web que abre la aplicación de escritorio y recupera la instancia `App` con `.value` al salir del bloque.

```python
import playwright_desktop

with page.expect_app(title="Información del equipo") as app_info:
    page.get_by_role("button", name="Obtener información").click()

app = app_info.value
app.get_by_role("textbox", name="Alias").fill("QA")
app.get_by_role("button", name="Confirmar").click()
app.wait_for_closed()  # espera a que la ventana se cierre tras confirmar
```

Playwright usa exactamente el mismo patrón con `page.expect_popup()` para abrir una pestaña o ventana nueva del navegador:

```python
with page.expect_popup() as popup_info:
    page.get_by_role("link", name="Abrir reporte").click()

popup = popup_info.value
```

Con Playwright asíncrono, usa `async with`, `app = await app_info.value` y `await` en las acciones sobre `App`, `Window`, `Alert` y `DesktopContext`.

## Ventanas abiertas y avisos intermedios

Mientras se abre la app principal dentro de `with page.expect_app(...)`, usa `app_info.context` (`DesktopContext`, compartido por todas las páginas del mismo `BrowserContext`) para interactuar con ventanas ya abiertas o diálogos intermedios:

```python
with page.expect_app(title="Información del equipo") as app_info:
    dcontext = app_info.context
    page.get_by_role("button", name="Obtener información").click()
    dcontext.app(title_prefix="Microsoft Edge").get_by_role("button", name="Abrir").click()
```

En `dcontext.app(...)` puedes filtrar por `title`, `title_prefix`, `pid`, `has_text` y `only_new=True` (ignora ventanas que ya estaban abiertas antes de iniciar el `DesktopContext`).

## Idioma del sistema operativo

`dcontext.os_language` y `playwright_desktop.os_language()` devuelven el idioma de interfaz del sistema en formato BCP-47 (por ejemplo `"es-ES"` o `"en-US"`):

```python
if dcontext.os_language.startswith("es"):
    dcontext.app(has_text="Seguridad de Windows", only_new=True).get_by_role("button", name="Ejecutar").click()
```

## Ventanas secundarias, alertas y cierre

Desde una `App` ya capturada puedes esperar sus propias ventanas hijas o alertas:

```python
with app.expect_alert(title="Confirmación") as alert_info:
    app.get_by_role("button", name="Guardar").click()

alert_info.value.get_by_role("button", name="Aceptar").click()
```

- `app.expect_window(...)` y `app.expect_alert(...)`: devuelven `Window` o `Alert` filtrados automáticamente al `pid` de `app`.
- `app.expect_child_app(...)`: captura otra `App` independiente lanzada desde `app`.
- `bring_to_front()`, `wait_for_closed()` y `close()`: disponibles en `App`, `Window` y `Alert`. Cerrar la parte desktop (`app.close()` o `dcontext.close()`) no cierra el navegador; al cerrar `page.context`, la sesión desktop se libera automáticamente.

## Configuración opcional

`playwright_desktop` es el módulo importado en tu script; úsalo solo si necesitas fijar valores globales en milisegundos o la ruta del JDK antes de iniciar sesión:

```python
import playwright_desktop

playwright_desktop.configure(timeout=15_000, java_home=r"C:\Program Files\Java\jdk-17")
```

También puedes cambiar el timeout de una sesión específica con `dcontext.set_default_timeout(15_000)`.
