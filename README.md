# playwright-desktop

Extiende el `Page` oficial de Playwright Python con `page.desktop` para interactuar con aplicaciones de escritorio.

## Instalación

**Pendiente de implementación:** `lib/` todavía contiene solo documentación. El comando de instalación local previsto, una vez implementado y empaquetado, es:

```powershell
py -m pip install --user ./lib
```

Los requisitos de construcción están definidos en la [arquitectura](docs/superpowers/specs/2026-11-17-libreria-python-rust-design.md#configuración-de-distribución-propuesta).

## Uso básico

La sintaxis definida se utiliza sobre un `page` que ya tengas creado con Playwright:

```python
import playwright_desktop

window = page.desktop.window(title="Mi aplicación")
window.get_by_role("textbox", name="Nombre").fill("Valor")
window.get_by_role("button", name="Aceptar").click()
```

Para Java en Windows, `JAVA_HOME` debe apuntar a un runtime OpenJDK con Java Access Bridge habilitado. También puedes indicar la ruta con `page.desktop.configure(java_home="C:/ruta/openjdk")`.

Con la API asíncrona de Playwright, utiliza `await` en acciones como `fill()` y `click()`. Los imports y las acciones web habituales se conservan.

## Estructura

```text
lib/     Librería Python y motor Rust
demo/    Ejemplos de integración y sus pruebas
docs/    Arquitectura y plan de implementación
```

## Más información

- [Demos: preparación, ejecución y pruebas](demo/README.md).
- [API y arquitectura](docs/superpowers/specs/2026-11-17-libreria-python-rust-design.md).
- [Plan de implementación](docs/superpowers/plans/2026-11-17-libreria-python-rust.md).
