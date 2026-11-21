# Diseño: Reestructuración DesktopContext y App acoplados a BrowserContext (Etapa 1)

**Fecha:** 2026-11-20
**Estado:** Aprobado
**Alcance:** Etapa 1 — Correlación `BrowserContext` ↔ `DesktopContext`, modelo `App` / `Window` / `Alert`, operaciones `bring_to_front` y `close`, y soporte multi-ventana en Python + Rust.

---

## 1. Propósito y Principios

Hoy la librería `playwright_desktop` expone un acceso plano `page.desktop` ligado al ciclo de vida de una sola `Page` y orientado a una única ventana capturada a la vez (`Window`). Esto dificulta modelar flujos donde:

1. Una aplicación web dentro de un `BrowserContext` abre una aplicación de escritorio (`App`), la cual puede abrir ventanas secundarias/alertas propias o lanzar otra aplicación hija en otro proceso, y luego el flujo retorna de forma continua a Playwright web.
2. El usuario cambia de pestaña/popup web (`page2`) dentro del mismo `BrowserContext` mientras el contexto de escritorio sigue vivo.

En esta **Etapa 1** reestructuramos la API y el runtime bajo cinco principios:

1. **`DesktopContext` depende de `BrowserContext`:** no se instancia manualmente ni existe `page.desktop`. Nace implícitamente cuando una `Page` invoca `page.expect_app(...)` y queda ligado al ciclo de vida de `page.context` (`BrowserContext`).
2. **Canal único Web → Desktop en `Page`:** el puente entre Playwright web y escritorio es `page.expect_app(...)`, que devuelve un `EventContextManager[App]` (sync) o `AsyncEventContextManager[App]` (async), análogo a `page.expect_popup()`.
3. **Semántica de escritorio, no `popup`:** una aplicación es `App` (equivalente a `Page`). Las ventanas secundarias, modales o diálogos dentro de esa misma `App` usan terminología de escritorio: `app.expect_window(...)` y `app.expect_alert(...)`. Abrir otra aplicación/proceso independiente usa `dcontext.expect_app(...)` o `app.expect_child_app(...)`.
4. **Cierre acotado sin matar el flujo web:** `app.close()` o `dcontext.close()` cierran únicamente la aplicación o la sesión de escritorio (como `page.close()` cierra una pestaña) y devuelven el flujo al `BrowserContext`/`Page` padre sin abortar la prueba.
5. **Compatibilidad multi-ventana en Rust dentro de una sesión:** un mismo `DesktopContext` puede coordinar varias `App`, `Window` y `Alert` registradas por handle sin perder la seguridad del worker único en Win32/UIA.

### Fuera de alcance en Etapa 1

- `DesktopContext` autónomo sin `BrowserContext` padre.
- Desacoplar `pyo3` del núcleo Rust para SDKs Node/TypeScript (N-API) o .NET/Java (queda para Etapa 2).

---

## 2. Mapeo Conceptual Playwright Web ↔ Escritorio

| Playwright (Web)          | Escritorio (Etapa 1)                                         | Descripción                                                                                    |
| ------------------------- | ------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| `BrowserContext`        | `DesktopContext`                                           | Entorno/sesión de escritorio ligado 1:1 al`BrowserContext` que lo originó.                  |
| `Page`                  | `App`                                                      | Aplicación/proceso (ventana principal por defecto).                                            |
| `page.<acción>`        | `app.<acción>`                                            | Acciones UIA sobre la ventana principal (`get_by_role`, `bring_to_front`, `close`, etc.). |
| `page.expect_popup()`   | `app.expect_window(...)` / `app.expect_alert(...)`       | Nueva ventana modal, diálogo o alerta dentro de la **misma `App`** (mismo PID).         |
| `context.expect_page()` | `dcontext.expect_app(...)` / `app.expect_child_app(...)` | Nueva `App` (otro proceso o aplicación lanzada desde una acción web o desktop).              |
| `page.bring_to_front()` | `app.bring_to_front()`                                     | Restaura si está minimizada y trae la ventana al frente.                                       |
| `page.close()`          | `app.close()`                                              | Cierra la ventana/aplicación de escritorio y continúa el flujo.                               |

---

## 3. Arquitectura y Ciclo de Vida Compartido

### 3.1 Parche sobre Playwright (`_patch.py`)

- Se parchean `playwright.sync_api.Page` y `playwright.async_api.Page` para exponer `page.expect_app(...)` en lugar de `page.desktop`.
- `install()` deja de registrar `DesktopAccessor` en `Page.desktop`.
- Si se requiere configuración global previa al primer `expect_app(...)` (como `java_home` o `timeout` por defecto), se expone `playwright_desktop.configure(java_home=..., timeout=...)` a nivel de módulo además de `timeout=` por llamada y `dcontext.set_default_timeout(...)`.

### 3.2 Correlación 1:1 `BrowserContext` ↔ `DesktopContext` (`_runtime.py`)

En lugar de almacenar `WeakKeyDictionary[Page, Session]` y escuchar `page.on("close")`:

1. Se mantiene un registro `WeakKeyDictionary[BrowserContext, DesktopSession]`.
2. Al llamar `page.expect_app(...)` sobre cualquier `page`:
   - Se obtiene `browser_context = page.context`.
   - Si no existe un `DesktopSession` activo para ese `browser_context`, se crea uno nuevo, guardando `weakref.ref(browser_context)`.
   - Se engancha `browser_context.on("close", ...)` y un `weakref.finalize(browser_context, ...)` para liberar recursos cuando muera el `BrowserContext`.
3. **Consecuencia práctica:**
   - Si durante el flujo desktop se abre una pestaña nueva (`page2`) o se cierra la `page` inicial mientras el `BrowserContext` siga vivo, el `DesktopContext` permanece intacto.
   - Al terminar la interacción con desktop (`app.close()` o `dcontext.close()`), el script continúa usando `page` (o `page2`) dentro del mismo `BrowserContext` sin reinicializar Playwright ni el navegador.
   - Al ejecutarse `browser_context.close()`, el `DesktopContext` asociado se cierra automáticamente (desconectando la sesión Rust/UIA y limpiando referencias).

```text
BrowserContext (Playwright)
   ├── Page (page) ──[page.expect_app(...)]──┐
   ├── Page (page2)                          ▼
   └── (1:1 lifecycle) ──────────────▶ DesktopContext (dcontext)
                                          ├── App (launcher)
                                          │    ├── app.expect_window(...)  ──▶ Window
                                          │    ├── app.expect_alert(...)   ──▶ Alert
                                          │    └── app.expect_child_app()  ──▶ App (java)
                                          └── dcontext.expect_app(...)     ──▶ App
```

### 3.3 Semántica de Cierre (`app.close()`, `window.close()`, `dcontext.close()`)

- **`app.close()` / `window.close()` / `alert.close()`:**
  - Envía cierre limpio Win32 (`WM_CLOSE`) al handle de esa ventana/app y espera a que cierre (dentro del timeout configurado, o si la app ya cerró no falla por ausencia del HWND).
  - Remueve esa entrada de `dcontext.apps` (o de las ventanas secundarias rastreadas).
  - **No cierra** ni el `BrowserContext` padre ni otros `Page` abiertos ni la sesión de `DesktopContext` (a menos que sea explícitamente `dcontext.close()`). El control regresa al flujo padre.
- **`dcontext.close()`:**
  - Cierra la sesión desktop asociada al `BrowserContext` (cierra ventanas/apps aún rastreadas por esa sesión si se indica `close_apps=True` o libera únicamente los handles en `False` — por defecto en `dcontext.close()` libera sesión sin matar procesos externos a menos que el test haya llamado `app.close()`, evitando dejar procesos colgados o romper inspecciones; ver detalle abajo).
  - **No cierra** el `BrowserContext` padre: simplemente devuelve el flujo al mundo web de Playwright. Si después el test vuelve a invocar `page.expect_app(...)`, nace un nuevo `DesktopContext` para ese `BrowserContext`.
- **Cierre automático por `browser_context.close()`:**
  - Ejecuta la limpieza equivalente a `dcontext.close()` sobre el `DesktopContext` hijo para evitar fugas de sesión o bloqueos en pruebas posteriores (`DesktopBusyError`).

---

## 4. Diseño de la API Pública (Python Sync & Async)

Se mantienen espejos simétricos en `_sync.py` y `_async.py` (las operaciones con IO/espera son síncronas en `_sync` y coroutines `async def` en `_async`, ejecutando llamadas bloqueantes de Rust en `asyncio.to_thread` como hasta hoy).

### 4.1 Puente en `Page`

```python
with page.expect_app(
    *,
    title: str | None = None,
    title_prefix: str | None = None,
    has_text: str | None = None,
    pid: int | None = None,
    predicate: Callable[[App], bool] | None = None,
    timeout: float | None = None,
) as app_info:
    page.get_by_role("link", name="Abrir Desktop").click()

app = app_info.value  # App
```

- Al entrar al `with` (`__enter__` / `__aenter__`):
  1. Obtiene o crea el `DesktopContext` desde `page.context` y lo expone inmediatamente en `app_info.context`.
  2. Toma snapshot de los HWNDs visibles actuales en la sesión Rust (`prepare`).
- Dentro del `with`: se ejecuta la acción web en Playwright (`click`, descarga ClickOnce, etc.). Si existen apps/ventanas intermedias del escritorio antes de que abra la `App` destino (p. ej. aviso nativo de Edge, diálogo de ClickOnce o SmartScreen), el analista puede acceder a ellas temporalmente vía **`app_info.context.app(...)`**, obteniendo objetos `App` completos con los mismos métodos (`.get_by_role(...)`, `.bring_to_front()`, `.close()`, etc.), sin distinción de tecnología ni API separada.
- Al salir del `with` (`__exit__` / `__aexit__`):
  - Si el bloque lanzó una excepción, la propaga sin enmascararla.
  - Si no, espera hasta `timeout` a que aparezca una nueva ventana de aplicación que cumpla `title`/`title_prefix`/`has_text`/`pid` (y `predicate(app)` si se proporcionó).
  - Registra la nueva `App` en `DesktopContext` y la expone en `app_info.value`.

> **Nota sobre variantes sin filtros o con `predicate`:** igual que `page.expect_popup()` en Playwright, si no se pasa filtro alguno, captura la primera ventana nueva de nivel superior que aparezca tras el snapshot; si se pasa `predicate`, evalúa la función sobre el candidato (`App`) antes de aceptarlo.

### 4.2 `DesktopContext`

Accesible vía `app.context` (o vía `app_info.context` durante un bloque `with page.expect_app(...) as app_info:`), nunca instanciado manualmente:

- `browser_context`: propiedad que retorna el `BrowserContext` padre.
- `apps`: tupla de `App` activas registradas en este contexto.
- `set_default_timeout(timeout: float)`: define timeout por defecto para operaciones de este contexto.
- `expect_app(title=..., title_prefix=..., has_text=..., pid=..., predicate=..., timeout=...)`: espera una nueva `App` abierta desde cualquier acción dentro del contexto (mismo mecanismo de snapshot/captura).
- `app(title=..., title_prefix=..., has_text=..., pid=..., timeout=...)`: localiza o referencia una `App` (por ejemplo, una app intermedia como Edge chrome, ClickOnce o SmartScreen durante un bloque `with page.expect_app(...)`) devolviendo siempre un objeto `App` con la misma superficie pública (`.get_by_role(...)`, `.bring_to_front()`, `.close()`, etc.).
- `close()`: cierra la sesión `DesktopContext` sin cerrar el `BrowserContext` padre.

### 4.3 `App`

Representa una aplicación (asociada a un `pid` y a su ventana principal `hwnd`):

- `context`: retorna el `DesktopContext` al que pertenece.
- `pid` / `title`: propiedades del proceso y título de la ventana principal al momento de captura.
- `get_by_role(role, *, name=..., exact=...)`: retorna un `Locator` UIA acotado a la ventana principal de la `App` (mismos métodos existentes: `get_by_role`, `wait_for`, `fill`, `input_value`, `click`).
- `bring_to_front()`: restaura (si está minimizada) y trae al frente la ventana principal de la `App`.
- `wait_for_closed(timeout=...)`: espera a que la ventana principal de la `App` se cierre por sí misma (ej. splash o launcher que se autocierra al abrir la app destino).
- `close(timeout=...)`: envía cierre limpio (`WM_CLOSE`) a la ventana de la `App`, espera su cierre, la remueve de `dcontext.apps` y devuelve el control al flujo llamador.
- `expect_child_app(...)`: atajo semántico sobre `app.context.expect_app(...)` para escenarios donde una `App` lanza otra `App` en un **proceso distinto** (p. ej. `InformationDemo.exe` → `javaw.exe`).
- `expect_window(title=..., title_prefix=..., has_text=..., predicate=..., timeout=...)`:
  - Espera una ventana secundaria/modal nueva que pertenezca al **mismo `pid`** de esa `App`.
  - Retorna un `EventContextManager[Window]` (o `AsyncEventContextManager[Window]`).
- `expect_alert(title=..., title_prefix=..., has_text=..., predicate=..., timeout=...)`:
  - Especialización de `expect_window(...)` acotada al **mismo `pid`** orientada a alertas o cuadros de diálogo modales (verifica que sea una ventana/diálogo hija o emergente del proceso de la `App` — opcionalmente filtrando por `has_text`/`title` del mensaje de alerta).
  - Retorna un `EventContextManager[Alert]` (o `AsyncEventContextManager[Alert]`).

### 4.4 `Window` y `Alert`

- `Window` representa una ventana secundaria de una `App` (hereda/comparte la misma superficie de interacción de ventana: `app` padre, `context`, `title`, `pid`, `get_by_role(...)`, `bring_to_front()`, `wait_for_closed()`, `close()`).
- `Alert` extiende `Window` con helpers orientados a alertas de escritorio:
  - `message()` / `text`: lee el texto visible principal del diálogo/alerta si es necesario inspeccionarlo sin localizar roles manualmente, manteniendo `get_by_role("button", name="OK")` / `close()` para interactuar y cerrarlo.

---

## 5. Cambios en el Motor Rust (`lib/rust/src/`)

Actualmente [engine.rs](../../../lib/rust/src/engine.rs) guarda un único `captured_window: Option<WindowHandle>` por sesión, lo cual impide tener activas al mismo tiempo una `App` padre, un `Window`/`Alert` modal y una `child App`.

### 5.1 Registro Multi-Handle por Sesión

- Reemplazar `captured_window: Option<WindowHandle>` en `Worker` por:
  - `active_session: Option<u64>` (mantiene exclusividad de sesión frente a otro `BrowserContext` simultáneo en el mismo proceso Python → `DesktopBusyError`, evitando interferencias Win32 globales).
  - `handles: HashMap<u64, WindowHandle>` (identificador entero opaco asignado por el engine al capturar cada ventana/app) y `next_handle_id: u64`.
- Las operaciones UIA/Win32 (`wait_visible`, `read_text`, `fill`, `click`, `wait_closed`, `bring_to_front`, `close_window`) pasan a recibir `handle_id: u64` junto con `session_id: u64` para operar exactamente sobre la `App`, `Window` o `Alert` correspondiente.
- `capture` recibe el `WindowQuery` (incluyendo filtro opcional `pid: Option<u32>` para `expect_window`/`expect_alert`) y retorna `{ handle_id, hwnd, pid, title }`.

### 5.2 Nuevas Operaciones Win32

En `Operation::parse` dentro de [engine.rs](../../../lib/rust/src/engine.rs) y módulo Win32 [windows.rs](../../../lib/rust/src/windows.rs):

1. **`bring_to_front`:**
   - Verifica validez de `hwnd` (`IsWindow`).
   - Si está minimizada (`IsIconic`), invoca `ShowWindow(hwnd, SW_RESTORE)`.
   - Llama `SetForegroundWindow(hwnd)` (y manejo de foco Win32 si es necesario para asegurar activación).
2. **`close_window`:**
   - Si `!IsWindow(hwnd)`, retorna OK (idempotente) y remueve el handle del mapa.
   - Envía `PostMessageW(hwnd, WM_CLOSE, 0, 0)`.
   - Espera dentro del `timeout` a que `!IsWindow(hwnd)` (reutilizando la espera de `wait_closed`) y remueve el handle del mapa al confirmarse el cierre.
   - Si la ventana sigue abierta tras `timeout` (p. ej. un diálogo "Guardar cambios" bloqueando el cierre), retorna error de timeout claro sin cerrar a la fuerza el proceso con `TerminateProcess`.

### 5.3 Gestión de Errores

- Se conservan las clases de error existentes (`DesktopError`, `DesktopBusyError`, `DesktopTimeoutError`, `DesktopWindowClosedError`, `DesktopUnsupportedPlatformError`, `DesktopVersionError`) añadiendo mensajes claros para `App`/`Window`/`Alert` cuando `handle_id` ya fue cerrado.

---

## 6. Impacto en Pruebas y Demo

1. **Tests unitarios e integración (`lib/tests/`):**
   - Actualizar `test_patch.py`: verificar que `Page` expone `expect_app` y ya **no** expone `desktop`; verificar correlación `BrowserContext` ↔ `DesktopContext` (incluyendo cambio a `page2` y cierre de `browser_context` disparando limpieza del `DesktopContext`).
   - Actualizar `test_api.py` y `test_async.py`: probar `page.expect_app(...)`, `app.context`, `app.expect_window(...)`, `app.expect_alert(...)`, `app.expect_child_app(...)`, `app.bring_to_front()`, `app.close()`, `dcontext.close()`, y continuación fluida sobre `page` tras cerrar `app`/`dcontext`.
   - Actualizar `test_selection.py`, `test_windows_uia.py`, `test_native.py` para el protocolo multi-handle (`handle_id`), `pid` filter, `bring_to_front` y `close_window`.
2. **Demo (`demo/pc-info-automation/`):**
   - Migrar los page objects y pruebas de `page.desktop.expect_window(...)` a:
     - `with page.expect_app(...) as app_info:` para capturar el launcher ClickOnce (`App`),
     - `with launcher.expect_child_app(...) as java_info:` para capturar la app Java (`App`),
     - `with java_app.expect_alert(...) as alert_info:` para validaciones de alertas/errores en Java,
     - `java_app.close()` (o cierre natural) retornando de forma continua a `page` en Edge.

---

## 7. Decisiones Confirmadas

1. **Convención Playwright en `Page`:** puente único `page.expect_app(...)` (sin `page.desktop` ni alias `expected_app`).
2. **Misma interfaz `App` para cualquier ventana de primer nivel (incluso intermedias):** `app_info.context` expone el `DesktopContext` (variable recomendada `dcontext`) al entrar al `with` para manejar apps intermedias mediante `dcontext.app(...) -> App` con `.get_by_role(...)`, antes de resolver `app_info.value`.
3. **Terminología de ventanas internas sin `popup`:** `app.expect_window(...)` y `app.expect_alert(...)` devuelven `Window` y `Alert` (mismo PID).
4. **Cierre acotado sin matar el flujo:** `app.close()` / `window.close()` / `alert.close()` envían `WM_CLOSE`; `dcontext.close()` (o cierre del `BrowserContext`) libera la sesión Rust/UIA y sus handles retornando al flujo padre en Playwright.
