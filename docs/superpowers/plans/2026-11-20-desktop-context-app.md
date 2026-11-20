# Plan de Implementación: DesktopContext y App acoplados a BrowserContext (Etapa 1)

> **Para el agente ejecutor:** Usa ejecución subagent-driven (`subagent-driven-development`) o paso a paso tarea por tarea (`executing-plans`) con TDD estricto. Los checks de verificación se marcan conforme pasan.

**Objetivo:** Reestructurar `playwright_desktop` (Rust + Python + Demo/Tests) para eliminar `page.desktop`, exponer `page.expect_app(...)` como canal único Web → Desktop donde nace `DesktopContext` acoplado al `BrowserContext` padre, modelar aplicaciones como `App` (con `bring_to_front()`, `close()`, `expect_window()`, `expect_alert()`, `expect_child_app()`), y devolver el flujo limpiamente a Playwright web al cerrar la parte desktop.

**Arquitectura:**
- **Núcleo Rust (`lib/rust/src/`):** Evoluciona `Worker` en `engine.rs` de un único slot `captured_window` a una tabla `handles: HashMap<u64, WindowHandle>` dentro de la sesión activa (`session_id`), agrega operaciones Win32 `bring_to_front` y `close_window` en `windows.rs` + `engine.rs`, y permite capturar ventanas filtrando por `pid` opcional (clave para `app.expect_window` / `app.expect_alert`) retornando `handle`, `pid` y `title` en `Reply`.
- **Runtime y Parche Python (`lib/python/playwright_desktop/`):** `_runtime.py` pasa el ciclo de vida de `WeakKeyDictionary[Page, Session]` a `WeakKeyDictionary[BrowserContext, DesktopSession]` usando `page.context`, `browser_context.on("close")` y `weakref.finalize`. `_patch.py` retira `page.desktop` y parchea `Page.expect_app` en `sync_api` y `async_api`. `_sync.py` y `_async.py` implementan `DesktopContext`, `App`, `Window`, `Alert` y los context managers `AppExpectation` / `WindowExpectation` / `AlertExpectation` (exponiendo `app_info.context` desde `__enter__`/`__aenter__` para apps intermedias como ClickOnce o SmartScreen mediante `dcontext.app(...) -> App`).
- **Tests y Demo (`lib/tests/` y `demo/pc-info-automation/`):** Migración completa de pruebas unitarias, integración Swing/Win32 UIA y flujo E2E de `demo/pc-info-automation` al nuevo modelo.

**Tech Stack:** Rust 2024 (`pyo3 = 0.30.1`, `windows = 0.62.2`, `maturin`), Python 3.10+ (`playwright >=1.63,<1.66`, `unittest`, `asyncio`).

**Referencia de diseño:** [2026-11-20-desktop-context-app-design.md](file:///d:/Workspace/Antigravity/QA%20Automation%20Interop/docs/superpowers/specs/2026-11-20-desktop-context-app-design.md)

---

### Task 1: Núcleo Rust — Registro multi-handle por sesión y metadatos en `Reply`

**Files:**
- Modify: `lib/rust/src/engine.rs`
- Modify: `lib/rust/src/windows.rs`
- Modify: `lib/python/playwright_desktop/_native.pyi`
- Test: `lib/rust/tests/` (o unit tests en `lib/rust/src/engine.rs`) y `lib/tests/test_native.py`

**Contexto clave:**
Hoy `engine.rs` almacena `captured_window: Option<windows::Window>` en `Worker` y `Reply` solo expone `handle: u64` y `text: Option<String>`. Para que un mismo `DesktopContext` coordine simultáneamente una `App`, sus ventanas secundarias (`Window` / `Alert`) y aplicaciones hijas (`child App`), el worker debe indexar múltiples handles por `handle_id: u64` sin perder exclusividad entre `BrowserContext` concurrentes (`DesktopBusyError`). Además, conocer `pid` y `title` en `Reply` permite que Python pueble `app.pid`, `app.title` y derive el filtro `pid` hacia `expect_window` / `expect_alert`.

- [x] **Step 1: Escribir tests unitarios en Rust para registro multi-handle y limpieza por sesión**

Agregar pruebas en el módulo `#[cfg(test)] mod tests` de `lib/rust/src/engine.rs` (siguiendo el estilo existente de unit tests en `engine.rs`) que verifiquen:
1. Dos handles distintos registrados en la misma `session_id` obtienen `handle_id` incrementales sin sobrescribirse y sin disparar `DesktopBusyError`.
2. Otra `session_id` distinta sigue fallando con `"BUSY"` mientras la sesión dueña siga viva.
3. Remover un `handle_id` (al cerrarlo) solo invalida ese handle dejando los demás vivos (`"STALE"` al invocar el cerrado, OK en el otro).

- [x] **Step 2: Ejecutar `cargo test` para verificar el fallo inicial (TDD)**

Run: `cargo test --manifest-path lib/rust/Cargo.toml --no-default-features` (usando `.tools/cargo/bin/cargo.exe` si no está en `PATH`, o igual que `lib/scripts/build_py.ps1`).
Expected: FAIL porque aún no existen `HandleEntry` / tabla multi-handle en `Worker`.

- [x] **Step 3: Actualizar `Reply` y la tabla de handles en `lib/rust/src/engine.rs` y `_native.pyi`**

1. En `lib/rust/src/engine.rs`:
   - Añadir campos opcionales a `Reply`: `pid: Option<u32>` y `title: Option<String>` con getters `#[getter]` en `#[pymethods] impl Reply`.
   - En `windows.rs`: agregar helper seguro `window_title(window: &Window) -> String` usando `GetWindowTextW` (reutilizando `Win32_UI_WindowsAndMessaging`) si no existe ya, y verificar que `Window` guarde/exponga `pid: u32`.
   - En `Worker`: reemplazar `captured_window: Option<windows::Window>` por:
     - `handles: HashMap<u64, WindowEntry>` (con `session_id: u64` y `window: windows::Window`)
     - `next_handle: u64` (iniciando en `1`).
   - En `Operation::Capture`: al capturar un `windows::Window` válido, registrar un nuevo `handle_id` en `self.handles` asociado a `session`, devolver `Reply { handle: handle_id, text: None, pid: Some(window.pid), title: Some(title) }`. (Si `prepare` no fue invocado antes para esa captura, mantener el comportamiento actual de `Invalid/Snapshot` o snapshot implícito por captura).
   - En operaciones con `window_id` (`WaitVisible`, `ReadText`, `Fill`, `Click`, `WaitClosed`): si `window_id != 0`, resolver exactamente `self.handles.get(&window_id)` validando que pertenezca a `session`. Para búsquedas sin captura previa (como `dcontext.app(...).get_by_role(...)`), seguir soportando resolución directa desde `WindowQuery` (`window_id == 0`).
   - Al cerrar sesión (`close_session` / `drop_session`), limpiar todas las entradas de `self.handles` cuya `session_id` coincida.
2. En `lib/python/playwright_desktop/_native.pyi`:
   - Declarar `@property def pid(self) -> int | None: ...` y `@property def title(self) -> str | None: ...` en `class Reply`.

- [x] **Step 4: Ejecutar `cargo test` y verificar que pasan las pruebas multi-handle**

Run: `& 'lib/.tools/cargo/bin/cargo.exe' test --manifest-path lib/rust/Cargo.toml`
Expected: PASS.

- [x] **Step 5: Commit parcial del núcleo multi-handle**

```bash
git add lib/rust/src/engine.rs lib/rust/src/windows.rs lib/python/playwright_desktop/_native.pyi
git commit -m "feat(rust): soportar múltiples handles de ventana por sesión"
```

---

### Task 2: Núcleo Rust — Operaciones Win32 `bring_to_front` y `close_window`

**Files:**
- Modify: `lib/rust/src/windows.rs`
- Modify: `lib/rust/src/engine.rs`
- Modify: `lib/tests/test_native.py`

**Contexto clave:**
`Operation::parse` hoy solo reconoce `prepare`, `capture`, `wait_visible`, `read_text`, `fill`, `click`, `wait_closed`. Necesitamos implementar las primitivas nativas equivalentes a `page.bring_to_front()` y `page.close()` sobre cualquier HWND capturado.

- [x] **Step 1: Escribir tests para parsing/validación de `bring_to_front` y `close_window` en `engine.rs` y `test_native.py`**

1. En `lib/rust/src/engine.rs`: probar que `Operation::parse` acepta `"bring_to_front"` y `"close_window"` y que ambos requieren `window_id` válido o `WindowQuery` válido (rechazan argumentos inválidos si ambos faltan).
2. En `lib/tests/test_native.py`: añadir prueba verificando error controlado (`StaleWindowError` / `NativeError("STALE", ...)`) cuando se llama `"bring_to_front"` sobre un handle inexistente, e idempotencia al llamar `"close_window"` sobre un handle ya cerrado si aplica o error esperado de argumento.

- [x] **Step 2: Ejecutar `cargo test` para verificar el fallo inicial**

Run: `& 'lib/.tools/cargo/bin/cargo.exe' test --manifest-path lib/rust/Cargo.toml`
Expected: FAIL porque aún no existen las variantes `BringToFront` y `CloseWindow`.

- [x] **Step 3: Implementar `bring_to_front` y `close_window` en `lib/rust/src/windows.rs` y `lib/rust/src/engine.rs`**

1. En `lib/rust/src/windows.rs`:
   - `pub(crate) fn bring_to_front(window: &Window) -> Result<()>`:
     - Si `!valid(window)`, retornar `Failure::new("STALE", "The desktop window is no longer open.")`.
     - Si `IsIconic(hwnd) != 0`, llamar `ShowWindow(hwnd, SW_RESTORE)`.
     - Llamar `SetForegroundWindow(hwnd)` (y `BringWindowToTop(hwnd)` como refuerzo sin fallar si Windows restringe cambio de foco entre procesos desatendidos, validando que `valid(window)` siga siendo verdadero).
   - `pub(crate) fn post_close(window: &Window) -> Result<bool>`:
     - Si `!valid(window)`, retornar `Ok(false)` (ya estaba cerrada).
     - Enviar `PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0))` (verificando firma exacta del crate `windows = 0.62.2` al compilar) y retornar `Ok(true)`.
2. En `lib/rust/src/engine.rs`:
   - Agregar `BringToFront` y `CloseWindow` a `enum Operation` y `Operation::parse`.
   - `BringToFront`: resuelve el `Window` (por `window_id` o `WindowQuery`), llama `windows::bring_to_front(&window)` y retorna `Reply` vacío.
   - `CloseWindow`: resuelve el `Window` (por `window_id` si existe; si ya fue cerrado o no está en `handles`, retornar OK idempotente); si está abierto, llama `windows::post_close(&window)` y espera dentro del `deadline` del `Request` hasta `!windows::valid(&window)` usando el loop cooperativo existente (respetando cancelación y retornando `"TIMEOUT"` si la app bloqueó el cierre tras el timeout). Al confirmarse el cierre, remueve `window_id` de `self.handles`.

- [x] **Step 4: Compilar wheel local y correr tests Rust + `test_native.py`**

Run:
```powershell
& 'lib/.tools/cargo/bin/cargo.exe' test --manifest-path lib/rust/Cargo.toml
lib/scripts/build_py.ps1
```
Expected: `cargo test` PASS y `maturin build` genera el wheel en `lib/dist/python/` sin warnings/errores de compilación.

- [x] **Step 5: Commit de las operaciones Win32**

```bash
git add lib/rust/src/windows.rs lib/rust/src/engine.rs lib/tests/test_native.py
git commit -m "feat(rust): implementar operaciones nativas bring_to_front y close_window"
```

---

### Task 3: Python Runtime y Parche — Ciclo de vida en `BrowserContext` y `page.expect_app(...)`

**Files:**
- Modify: `lib/python/playwright_desktop/_runtime.py`
- Modify: `lib/python/playwright_desktop/_patch.py`
- Modify: `lib/python/playwright_desktop/_models.py`
- Modify: `lib/python/playwright_desktop/__init__.py`
- Modify: `lib/python/playwright_desktop/typing.py`
- Modify: `lib/tests/test_patch.py`

**Contexto clave:**
Hoy `_patch.py` inyecta `DesktopAccessor` en `Page.desktop`, y `_runtime.py` asocia una `Session` a `weakref.ref(page)` escuchando `page.on("close", ...)`. Debemos:
1. Mover la propiedad del ciclo de vida al `BrowserContext` (`page.context`), manteniendo `WeakKeyDictionary[BrowserContext, Session]`.
2. Retirar `Page.desktop` por completo e inyectar el método `expect_app` en `SyncPage` y `AsyncPage` (`_patch.py`), además de exponer `configure(java_home=..., timeout=...)` a nivel de paquete (`__init__.py`) para configuración previa opcional.

- [ ] **Step 1: Actualizar `lib/tests/test_patch.py` con las expectativas del nuevo contrato (TDD)**

Actualizar `FakePage` agregando `FakeContext` (`self.context = context or FakeContext()`, con `.on(event, callback)`, `.is_closed()` y `.close()` disparando `"close"`). Probar que:
1. `install()` agrega `"expect_app"` a `SyncPage` y `AsyncPage`, y **NO** agrega `"desktop"`.
2. Colisión (`PatchConflictError`), rollback en falla parcial, idempotencia de `reload`, `uninstall()`, y carga perezosa de `_native` siguen cumpliéndose igual para `"expect_app"`.
3. `Session` / `DesktopContext` se comparte entre `page1` y `page2` del **mismo** `FakeContext`, no muere al cerrarse `page1` si el `BrowserContext` sigue vivo, y sí se cierra al ejecutarse `context.callbacks["close"]()` o recolectarse el `BrowserContext` (`weakref`).

- [ ] **Step 2: Ejecutar `test_patch.py` para verificar fallo esperado**

Run: `py -m unittest discover -s lib/tests -p "test_patch.py" -v` (ajustando `PYTHONPATH=lib/python` si corresponde).
Expected: FAIL mientras `_patch.py` y `_runtime.py` sigan usando `desktop` y `Page`.

- [ ] **Step 3: Refactorizar `_runtime.py`, `_models.py`, `_patch.py`, `__init__.py` y `typing.py`**

1. **`lib/python/playwright_desktop/_models.py`:**
   - Permitir que `WindowQuery` acepte filtro `predicate` separado en capa Python o query completamente abierta cuando `expect_app()` / `expect_window()` / `expect_alert()` se llama sin criterios declarativos (evaluando cualquier ventana nueva capturada en el backend o adaptando `WindowQuery.optional(...)` para `expect_*` sin filtros). Mantener validación estricta para `dcontext.app(...)` directo.
   - Mantener `validate_timeout`, `validate_control`, `validate_text` con las mismas reglas existentes.
2. **`lib/python/playwright_desktop/_runtime.py`:**
   - Guardar configuración por defecto global (`_DEFAULT_CONFIG`: `java_home`, `timeout`) configurable vía `configure_defaults(java_home=..., timeout=...)`.
   - Renombrar/evolucionar `Session` para recibir `browser_context`:
     - `self._browser_context = weakref.ref(browser_context)` y propiedad `browser_context` (lanza `DesktopClosedError` si el contexto padre ya murió).
     - En `__init__`: `browser_context.on("close", on_close)` y `weakref.finalize(browser_context, self.close_nowait)`.
     - Rastrear apps/ventanas registradas en la sesión (`register_window`, `unregister_window`, `tracked_apps`).
     - Proveer factory helper `session_for_context(browser_context)` respaldada por `WeakKeyDictionary[object, Session]` (garantizando un único `DesktopContext`/sesión por `BrowserContext`).
     - `close_nowait()` limpia las referencias en la sesión, cierra la sesión nativa en Rust sin afectar al `BrowserContext` padre.
3. **`lib/python/playwright_desktop/_patch.py`:**
   - Cambiar atributo parcheado de `"desktop"` a `"expect_app"`.
   - Implementar descriptor o método puente en `_patch.py` para `"sync"` y `"async"` que delegue `self.context` al `DesktopContext` correspondiente y retorne `AppExpectation` (sync) / `AsyncAppExpectation` (async).
   - Eliminar `DesktopAccessor` / referencias a `Page.desktop`.
4. **`lib/python/playwright_desktop/__init__.py` y `typing.py`:**
   - Exportar `DesktopContext`, `App`, `Window`, `Alert` (además de `configure`, `install`, `uninstall`, y los errores públicos).
   - Actualizar `typing.py` con `HasExpectApp` / `HasAsyncExpectApp` si los usuarios usan tipado estructural.

- [ ] **Step 4: Ejecutar `test_patch.py` y verificar PASS**

Run: `$env:PYTHONPATH="lib/python"; py -m unittest lib/tests/test_patch.py -v`
Expected: PASS en todos los casos de parcheo, colisión, rollback, carga perezosa y ciclo de vida en `BrowserContext`.

- [ ] **Step 5: Commit de runtime, parche y modelos**

```bash
git add lib/python/playwright_desktop/_runtime.py lib/python/playwright_desktop/_patch.py lib/python/playwright_desktop/_models.py lib/python/playwright_desktop/__init__.py lib/python/playwright_desktop/typing.py lib/tests/test_patch.py
git commit -m "feat(py): ligar DesktopContext a BrowserContext y exponer page.expect_app"
```

---

### Task 4: Python Sync & Async API — `DesktopContext`, `App`, `Window`, `Alert` y Expectations

**Files:**
- Modify: `lib/python/playwright_desktop/_sync.py`
- Modify: `lib/python/playwright_desktop/_async.py`
- Modify: `lib/tests/test_api.py`
- Modify: `lib/tests/test_async.py`

**Contexto clave:**
Debemos construir las dos implementaciones paralelas (`_sync.py` y `_async.py`) manteniendo duplicación mínima de lógica compartida en `_models.py` / `_runtime.py`, asegurando que:
- `with page.expect_app(...) as app_info:` crea/recupera el `DesktopContext` en `__enter__`, expone `app_info.context` durante el bloque `with` (para que cualquier ventana intermedia se obtenga vía `app_info.context.app(...) -> App`), toma el snapshot (`prepare`), y en `__exit__` captura la nueva ventana retornando `App` en `app_info.value`.
- `App` expone: `.context`, `.pid`, `.title`, `.get_by_role(...)`, `.bring_to_front()`, `.wait_for_closed()`, `.close()`, `.expect_window(...)` → `Window`, `.expect_alert(...)` → `Alert` (acotados automáticamente al `pid` de esa `App`), `.expect_child_app(...)` → `App`.
- `Window` y `Alert` comparten interfaz completa sobre su `handle_id` (`.app`, `.context`, `.pid`, `.title`, `.get_by_role(...)`, `.bring_to_front()`, `.wait_for_closed()`, `.close()`), con helper `.text` / `.message()` en `Alert`.
- `DesktopContext` expone `.browser_context`, `.apps`, `.set_default_timeout(...)`, `.app(...) -> App`, `.expect_app(...) -> EventContextManager[App]`, `.close()`.
- Cerrar `app.close()`, `window.close()`, `alert.close()` o `dcontext.close()` cierra únicamente lo correspondiente en desktop y nunca cierra el `BrowserContext` padre.

- [ ] **Step 1: Reescribir/ampliar `lib/tests/test_api.py` y `lib/tests/test_async.py` según el nuevo contrato**

Incluir pruebas unitarias (con fake engine / mocks) cubriendo:
1. `with page.expect_app(title="...") as info:` expone `info.context` (`DesktopContext`) dentro del `with`, devuelve `App` en `info.value`, con `app.context.browser_context is page.context` y `app in app.context.apps`.
2. Ventana intermedia dentro del `with` (`intermediate = info.context.app(has_text="SmartScreen")`) retorna un `App` completo con `.get_by_role(...)`.
3. `app.expect_window(...)` y `app.expect_alert(...)` pasan automáticamente `pid=app.pid` a `prepare`/`capture` y devuelven `Window` / `Alert` respectivamente; `app.expect_child_app(...)` no fija `pid` padre por defecto y devuelve otra `App`.
4. `app.bring_to_front()` invoca `"bring_to_front"` con `window_id=app._handle`; `app.close()` invoca `"close_window"` con `window_id=app._handle`, remueve `app` de `dcontext.apps`, deja vivo `dcontext` y `page.context`.
5. `dcontext.close()` cierra la sesión nativa sin llamar `browser_context.close()`.
6. Versión `async` completa en `test_async.py`: `async with page.expect_app(...) as info:`, `await info.value`, cancelación de tarea nativa durante `await app.get_by_role(...).input_value()`, `await app.bring_to_front()`, `await app.close()`, cierre de `BrowserContext` liberando sesión.

- [ ] **Step 2: Ejecutar `test_api.py` y `test_async.py` para ver los fallos iniciales**

Run: `$env:PYTHONPATH="lib/python"; py -m unittest lib/tests/test_api.py lib/tests/test_async.py -v`
Expected: FAIL porque `_sync.py` y `_async.py` todavía exponen `Desktop`/`expect_window` antiguo.

- [ ] **Step 3: Implementar `DesktopContext`, `App`, `Window`, `Alert` y `Expectation` en `_sync.py` y `_async.py`**

Puntos críticos de calidad e implementación:
- Extraer lógica compartida de filtrado `predicate` y normalización de parámetros de consulta hacia `_models.py` para no duplicar entre `_sync.py` y `_async.py`.
- Manejar limpiamente `predicate` cuando es pasado: si la ventana capturada por query base no satisface `predicate(candidate)`, permitir reintento o documentar claramente cómo opera sobre candidatos capturados.
- En `expect_app` / `expect_window` / `expect_alert`: si `query` no especifica `title`/`title_prefix`/`has_text`/`pid`, permitir captura de la primera ventana nueva detectada por diferencia contra el snapshot de `prepare` (asegurando en Rust que `prepare` + `capture` soporten query abierta o al menos `pid` de la `App` padre en `expect_window`/`expect_alert`!). *(Nota: verificar si `engine.rs` / `_models.py` hoy exige al menos un campo en `WindowQuery` cuando `expect_app()` se llama sin argumentos; si es necesario permitir `WindowQuery` abierto para `expect_app()`, ajustar `WindowQuery::validate` en `engine.rs` y `_models.py` manteniendo la validación para `dcontext.app(...)`).*
- En `Alert.message()` / `Alert.text`: leer texto visible principal de diálogo si aplica (o texto del panel/control de texto principal) reutilizando el canal de lectura UIA existente.

- [ ] **Step 4: Recompilar si hubo ajuste en `WindowQuery` de Rust y ejecutar todas las pruebas unitarias Python**

Run:
```powershell
$env:PYTHONPATH="lib/python"; py -m unittest lib/tests/test_patch.py lib/tests/test_api.py lib/tests/test_async.py -v
```
Expected: PASS 100% sin tests saltados por mocks rotos.

- [ ] **Step 5: Commit de la nueva API Sync & Async**

```bash
git add lib/python/playwright_desktop/_sync.py lib/python/playwright_desktop/_async.py lib/python/playwright_desktop/_models.py lib/tests/test_api.py lib/tests/test_async.py
git commit -m "feat(py): implementar DesktopContext, App, Window y Alert en sync y async"
```

---

### Task 5: Migrar pruebas de integración (`test_selection.py`, `test_windows_uia.py`) y compilar wheel final

**Files:**
- Modify: `lib/tests/test_selection.py`
- Modify: `lib/tests/test_windows_uia.py`

**Contexto clave:**
Estas suites prueban contra ventanas reales de Java Swing (`SelectionForm.java`) y Win32/UIA nativo en Windows x64. Hay que adaptarlas al nuevo API (`FakeContext`, `playwright_desktop.configure(...)`, `expect_app`, `App`, `expect_window` / `expect_alert`, `bring_to_front`, `close`) y añadir pruebas reales de cierre de ventana y foco.

- [ ] **Step 1: Actualizar `lib/tests/test_selection.py` y `lib/tests/test_windows_uia.py`**

1. Reemplazar `Desktop(Session(page))` / `page.desktop.expect_window(...)` por `FakeContext` + `DesktopContext` (o `page.expect_app(...)` real usando el parche instalado sobre un `FakePage` con `context=FakeContext()`).
2. Agregar un caso en `test_windows_uia.py` que abra una ventana nativa real de prueba, valide `app.bring_to_front()` y `app.close()`, verificando que la ventana se cierra limpiamente y `app.context.browser_context.is_closed()` sigue siendo `False`.

- [ ] **Step 2: Compilar wheel release e instalar en entorno temporal para ejecutar toda la suite `lib/tests`**

Run:
```powershell
lib/scripts/build_py.ps1
```
Y ejecutar `unittest discover` sobre `lib/tests` apuntando al wheel recién instalado (y con `PLAYWRIGHT_DESKTOP_TEST_JAVA_HOME` configurado si está disponible el JDK en la máquina para no saltar `test_selection.py`).
Expected: PASS en `test_patch.py`, `test_api.py`, `test_async.py`, `test_native.py`, `test_windows_uia.py` (y `test_selection.py` con JDK).

- [ ] **Step 3: Commit de tests de integración actualizados**

```bash
git add lib/tests/test_selection.py lib/tests/test_windows_uia.py
git commit -m "test(lib): migrar pruebas de selección y UIA a DesktopContext y App"
```

---

### Task 6: Migrar Demo `demo/pc-info-automation` y validar flujo E2E real

**Files:**
- Modify: `demo/pc-info-automation/pages/browser_clickonce.py`
- Modify: `demo/pc-info-automation/pages/clickonce_installer.py`
- Modify: `demo/pc-info-automation/pages/sscreen_warning.py`
- Modify: `demo/pc-info-automation/pages/information_form.py`
- Modify: `demo/pc-info-automation/tests/test_flow.py`
- Modify: `demo/pc-info-automation/tests/test_clickonce.py`
- Modify: `demo/pc-info-automation/fixtures/demo.py` (revisar si configura `java_home` o `page.desktop`)

**Contexto clave:**
En la demo real:
- `Edge` dispara ClickOnce → aviso nativo de Edge (`BrowserClickOnce`) → instalador ClickOnce (`ClickOnceInstaller`) → aviso opcional SmartScreen (`SScreenWarning`) → aplicación Java (`InformationForm`) → cierre Java → continuación fluida en Edge para validar el mensaje, tabla y PDF generado.
- Todas las interacciones intermedias dentro del `with page.expect_app(...) as app_info:` pasan a recibir `app_info.context` (`DesktopContext`) y adquieren un objeto `App` vía `.app(...)`, sin usar `page.desktop` en ningún archivo.

- [ ] **Step 1: Inspeccionar `demo/pc-info-automation/fixtures/demo.py` para detectar cualquier uso de `page.desktop`**

Read lines 1-100 of `demo/pc-info-automation/fixtures/demo.py` before editing.

- [ ] **Step 2: Migrar page objects de `demo/pc-info-automation/pages/`**

1. `BrowserClickOnce`: aceptar `(dcontext: DesktopContext, page: Page)` y reemplazar `self.page.desktop.window(...)` por `self.dcontext.app(pid=pid, title_prefix=title_prefix)`.
2. `ClickOnceInstaller`: aceptar `(dcontext: DesktopContext, ...)` y usar `dcontext.app(title=...)`.
3. `SScreenWarning`: aceptar `(dcontext: DesktopContext, page: Page)` y usar `dcontext.app(has_text=...)`.
4. `InformationForm`: renombrar parámetro/atributo `window` a `app` (`App`) usando `self.app.get_by_role(...)` y `self.app.wait_for_closed()` (más `close()` si aplica en futuras pruebas).

- [ ] **Step 3: Migrar `test_flow.py`, `test_clickonce.py` y `fixtures/demo.py`**

1. En `test_flow.py`:
   ```python
   with page.expect_app(
       title=InformationForm.TITLE, timeout=ACTIVATION_TIMEOUT_MS
   ) as opened:
       dcontext = opened.context
       dcontext.set_default_timeout(15_000)
       browser = BrowserClickOnce(dcontext, page)
       installer = ClickOnceInstaller(dcontext, application_name=InformationForm.TITLE, base_url=self.web.base_url)
       sscreen = SScreenWarning(dcontext, page)
       form_ready = dcontext.app(title=InformationForm.TITLE).get_by_role("button", name="Confirmar")
       ...
   app = opened.value  # App directo (estilo Playwright: popup = popup_info.value)
   form = InformationForm(app)  # Page Object de la demo envolviendo el App
   ```
   Luego al confirmar el formulario Java, verificar que el flujo retorna limpiamente a `self.web` (`self.page` en el mismo `BrowserContext`) y completa la validación del PDF.
2. En `test_clickonce.py`: envolver en `with page.expect_app(title_prefix="Ejecución de la aplicación", ...)` (o equivalente EN/ES) usando `app_info.context` para `BrowserClickOnce` y cerrar limpiamente el diálogo capturado con `app_info.value.close()`.
3. Confirmar por grep que no queda ninguna referencia a `page.desktop` en `demo/`.

- [ ] **Step 4: Validar ejecución E2E de `demo/pc-info-automation/run_local.ps1`**

Con los servidores locales de la demo levantados (`demo/pc-info/start.ps1`):
Run: `demo/pc-info-automation/run_local.ps1`
Expected: `CP-PI-01` (y `CP-PI-02`) ejecutan completo hasta descargar y verificar el PDF sin errores y sin dejar procesos colgados.

- [ ] **Step 5: Commit de migración de la demo**

```bash
git add demo/pc-info-automation/
git commit -m "refactor(demo): migrar pc-info-automation a page.expect_app, DesktopContext y App"
```

---

### Task 7: Documentación técnica y verificación final del repositorio

**Files:**
- Modify: `lib/README.md`
- Modify: `docs/documentacion-tecnica.md` (si documenta `page.desktop`)

- [ ] **Step 1: Actualizar `lib/README.md` y documentación con ejemplos del nuevo contrato `page.expect_app` / `DesktopContext` / `App` / `Window` / `Alert`**
- [ ] **Step 2: Ejecutar verificación integral (`cargo test` + `build_py.ps1` + suite `lib/tests`) y `git status` limpio**
- [ ] **Step 3: Commit final de documentación**

```bash
git add lib/README.md docs/documentacion-tecnica.md
git commit -m "docs(lib): actualizar documentación para DesktopContext y App"
```
