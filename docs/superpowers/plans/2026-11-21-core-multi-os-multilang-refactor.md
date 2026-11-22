# Plan de Refactorización: Núcleo Multi-SO y Multi-Lenguaje (Cargo Workspace)

**Goal:** Desacoplar el motor nativo de Python/PyO3 dentro de un Cargo Workspace y abstraer la configuración de sesión, los proveedores de accesibilidad y la detección de idioma del SO en Rust puro para escalar a futuros SO (macOS/Linux), tecnologías de UI y lenguajes cliente (TypeScript, etc.), sin implementar aún esos nuevos targets ni romper el contrato Python/Windows vigente.

**Architecture:** Reorganizar `lib/` en un Cargo Workspace (`lib/Cargo.toml`) con un crate núcleo puro `playwright-desktop-core` (`lib/core/`, sin dependencia de `pyo3`) y un adaptador delgado `playwright-desktop-py` (`lib/bindings/python/`, solo FFI PyO3). Dentro del núcleo se separa la fachada pública del coordinador de estado (`engine/coordinator.rs`), se reemplazan campos fijos de Java por `SessionConfig` extensible, se introduce el trait `WindowsAccessibilityProvider` con `ProviderRegistry` (Open/Closed) y se traslada `os_language()` al núcleo `Platform`. La capa Python existente (`_sync.py`, `_async.py`, `_facade.py`, `_runtime.py`, `_patch.py`) conserva exactamente la misma API pública.

**Tech Stack:** Rust 2024 (`rust-version = "1.85"`, `windows = "=0.62.2"`, `pyo3 = "=0.30.1"` únicamente en `bindings/python`), Maturin (`>=1.16,<2`), Python 3.10–3.14 (`unittest`, `playwright >=1.63,<1.66`).

**Referencia de diseño:** [2026-11-20-desktop-context-app-design.md](../specs/2026-11-20-desktop-context-app-design.md)

## Global Constraints

- **Cero ruptura de contrato Python/Windows:** `page.expect_app`, `DesktopContext`, `App`, `Window`, `Alert`, `only_new`, `os_language`, `configure(...)`, códigos de `NativeError` (`CONFIGURATION`, `BRIDGE`, `CANCELLED`, `CLOSED`, `STALE`, `BUSY`, `TIMEOUT`, `ACTION`) y la demo `pc-info-automation` deben seguir funcionando idénticos.
- **YAGNI estricto:** NO implementar bindings de TypeScript/Node ni adaptadores de macOS/Linux en esta etapa; dejar únicamente los puertos, traits y límites de crate listos.
- **Cero PyO3 en el núcleo:** `cargo tree -p playwright-desktop-core` no debe incluir `pyo3`, ni mensajes específicos de un lenguaje cliente (`"Restart Python"` → `"Restart the host process"`).
- **Integridad de empaquetado:** `.\lib\scripts\build_py.ps1` debe generar `.whl` y `.tar.gz` (sdist) con `--locked` empaquetando todo el Cargo Workspace de `lib/`.
- **Documentación y seguridad:** Prohibido incluir rutas locales absolutas o referencias a credenciales locales en código o documentación.

## Review Focus

1. **Carga perezosa de `_native` al importar y en `os_language()`:** `import playwright_desktop` no debe importar `_native`, y `playwright_desktop.os_language()` en sistemas no soportados (o sin extensión cargada durante tests unitarios mockeados) debe delegar en Rust cuando el binario nativo está presente sin romper el fallback con `locale.getlocale()` (cubierto en Task 3).
2. **Empaquetado reproducible de `sdist` (`maturin sdist`) tras pasar a Workspace:** Mover `Cargo.toml`/`Cargo.lock` implica que `.tar.gz` debe incluir `Cargo.toml`, `Cargo.lock`, `core/**` y `bindings/python/**` para compilar desde fuentes con `--locked` (cubierto en Tasks 4 y 5).
3. **Preservación exacta de errores y firmas de `_native.Engine`:** El cambio interno a `SessionConfig` e `InvokeParams` en el núcleo no debe alterar los códigos ni los textos validados por `lib/python/tests/test_native.py` como `"java_home"` en `open_session("")` (cubierto en Tasks 1 y 2).
4. **Aislamiento de fallos y fallback en `ProviderRegistry` extensible:** Si un proveedor primario falla o está bloqueado en modo automático, el registro debe respetar el fallback (UIA) sin bloquear el hilo coordinador ni contaminar otros proveedores (cubierto en Task 2).
5. **Compilación y tests autónomos de `playwright-desktop-core`:** El crate `lib/core` debe compilar y pasar sus pruebas unitarias en Rust sin tener Python ni PyO3 enlazados (cubierto en Task 4).

---

## Estructura de Archivos Objetivo

```text
lib/
├── Cargo.toml                          # Workspace Cargo: members = ["core", "bindings/python"]
├── Cargo.lock                          # Lockfile unificado del workspace
├── pyproject.toml                      # readme = "python/README.md", manifest-path = "bindings/python/Cargo.toml"
├── core/
│   ├── Cargo.toml                      # Crate Rust puro "playwright-desktop-core" (sin pyo3)
│   ├── tests/abi_layout.c              # Validación ABI existente (trasladado desde rust/tests/)
│   └── src/
│       ├── lib.rs                      # API Rust pública de dominio (CoreEngine, CoreRequest, CoreReply, os_language)
│       ├── error.rs                    # Failure y Result<T> independientes del lenguaje cliente
│       ├── config.rs                   # SessionConfig extensible y ProviderConfig
│       ├── engine.rs                   # Fachada pura del motor, comandos, colas, espera y SessionState
│       ├── engine/
│       │   ├── coordinator.rs          # Worker loop multi-handle, baselines y ciclo de captura
│       │   ├── platform.rs             # Trait Platform (+ os_language), WindowQuery, ControlQuery
│       │   ├── providers.rs            # Trait WindowsAccessibilityProvider y ProviderRegistry (OCP)
│       │   ├── windows_platform.rs     # Adaptador WindowsPlatform delegando en ProviderRegistry y Win32
│       │   └── tests.rs                # Tests unitarios puros (FakePlatform + pruebas de ProviderRegistry)
│       ├── backend.rs                  # Adaptador JAB conectado vía JavaProviderAdapter
│       ├── windows.rs                  # EnumWindows, foco, cierre y host_os_language() (Win32_Globalization)
│       ├── uia.rs                      # Cliente y acciones UI Automation
│       └── jab/                        # FFI Java Access Bridge
├── bindings/
│   └── python/
│       ├── Cargo.toml                  # Crate cdylib "_native" dependiente de core + pyo3
│       └── src/
│           └── lib.rs                  # Adaptador delgado PyO3 (Engine, Request, Reply, os_language, NativeError)
├── python/
│   ├── README.md                       # Movido desde lib/README.md
│   ├── playwright_desktop/             # Paquete Python (solo ajuste en _backend.py, _platform.py, _native.pyi)
│   └── tests/                          # Movido desde lib/tests/ (unittest / Swing / UIA)
└── scripts/build_py.ps1                # Compila --release --locked y genera sdist desde workspace lib/
```

---

### Task 1: Desacoplar el dominio puro Rust de PyO3 dentro de `lib/rust/`

**Files:**
- Create: `lib/rust/src/engine/coordinator.rs`
- Create: `lib/rust/src/bindings_py.rs`
- Modify: `lib/rust/src/lib.rs`
- Modify: `lib/rust/src/engine.rs`
- Modify: `lib/rust/src/engine/platform.rs`
- Modify: `lib/rust/src/engine/windows_platform.rs`
- Test: `lib/rust/src/engine/tests.rs` y `lib/tests/test_native.py`

**Interfaces:**
- Consumes: tipos internos actuales de `engine.rs`, `platform.rs`, `error.rs`.
- Produces en Rust puro (sin tipos de `pyo3`):
  - `pub struct CoreEngine` (`new() -> Result<Self>`, `open_session(&self, options: SessionOptions) -> Result<u64>`, `new_request(&self, session_id: u64, timeout_ms: u64) -> Result<CoreRequest>`, `invoke(&self, request: &CoreRequest, params: InvokeParams) -> Result<CoreReply>`, `close_session(&self, session_id: u64)`, `cancel_capture(&self, session_id: u64, capture_id: u64)`)
  - `pub struct CoreRequest` (`cancel(&self)`)
  - `pub struct CoreReply` (`handle: u64`, `text: Option<String>`, `pid: Option<u32>`, `title: Option<String>`)
  - `pub struct InvokeParams` (agrupa `operation`, `window_id`, `capture_id`, `window: WindowQuery`, `text`, `control: ControlQuery`)
  - Mensajes de error agnósticos al lenguaje anfitrión (`"Restart the host process."` en lugar de `"Restart Python."`).

- [ ] **Step 1: Escribir tests unitarios puros para `CoreEngine`, `InvokeParams` y mensajes agnósticos al lenguaje**

Agregar en `lib/rust/src/engine/tests.rs`:
```rust
#[test]
fn core_engine_error_messages_do_not_reference_client_runtime() {
    let message = crate::engine::worker_stopped_failure().message;
    assert!(!message.contains("Python"));
    assert!(message.contains("Restart the host process"));
}

#[test]
fn invoke_params_rejects_unknown_operation_without_python_context() {
    let err = Operation::parse("unsupported").err().unwrap();
    assert_eq!(err.code, "CONFIGURATION");
}
```

- [ ] **Step 2: Ejecutar `cargo test` para verificar el fallo inicial (TDD)**

Run:
```powershell
$env:CARGO_HOME = (Resolve-Path 'lib/.tools/cargo').Path
$env:RUSTUP_HOME = (Resolve-Path 'lib/.tools/rustup').Path
& "$env:CARGO_HOME/bin/cargo.exe" test --manifest-path lib/rust/Cargo.toml
```
Expected: FAIL porque aún no existen `worker_stopped_failure()` ni las estructuras desacopladas de PyO3.

- [ ] **Step 3: Extraer `Worker` a `engine/coordinator.rs`, limpiar `engine.rs` de `pyo3` y mover wrappers PyO3 a `bindings_py.rs`**

1. Mover `Worker`, `ActiveWindow`, `Closed` / manejo de `handles`, `baselines`, `pending`, `ensure_baseline`, `prepare_capture`, `complete_capture`, `act_on_window`, `begin_close` y `poll_closing` desde `lib/rust/src/engine.rs` hacia `lib/rust/src/engine/coordinator.rs` (~350 líneas enfocadas en coordinación).
2. Dejar `lib/rust/src/engine.rs` 100% libre de `use pyo3::...`:
   - Exponer `pub struct CoreEngine`, `pub struct CoreRequest`, `pub struct CoreReply` y `pub struct InvokeParams`.
   - Reemplazar todas las cadenas `"Restart Python..."` en `engine.rs` y `windows_platform.rs` por `"Restart the host process..."` mediante helper compartido (`worker_stopped_failure()`, `unavailable()`).
3. Crear `lib/rust/src/bindings_py.rs` con `#[pyclass] struct Engine(CoreEngine)`, `#[pyclass] struct Request(CoreRequest)`, `#[pyclass] struct Reply`, manteniendo las firmas exactas que espera Python (`open_session`, `open_auto_session`, `new_request`, `invoke` liberando el GIL con `py.detach`, `close_session`, `cancel_capture`, getters de `Reply`).
4. Actualizar `lib/rust/src/lib.rs` para declarar `mod bindings_py;` (manteniendo `#[pymodule] fn _native`).

- [ ] **Step 4: Verificar compilación Rust + pruebas unitarias Rust y `test_native.py`**

Run:
```powershell
& "$env:CARGO_HOME/bin/cargo.exe" test --manifest-path lib/rust/Cargo.toml
```
Expected: PASS (todos los tests de Rust existentes + los nuevos).

- [ ] **Step 5: Commit de separación dominio/FFI**

```bash
git add lib/rust/src/
git commit -m "refactor(rust): separar motor puro de adaptador PyO3 y lenguaje cliente"
```

---

### Task 2: Configuración extensible por proveedor (`SessionConfig`) y registro abierto de proveedores (`ProviderRegistry`)

**Files:**
- Create: `lib/rust/src/config.rs`
- Create: `lib/rust/src/engine/providers.rs`
- Modify: `lib/rust/src/lib.rs`
- Modify: `lib/rust/src/engine.rs`
- Modify: `lib/rust/src/engine/platform.rs`
- Modify: `lib/rust/src/engine/windows_platform.rs`
- Modify: `lib/rust/src/backend.rs`
- Modify: `lib/rust/src/bindings_py.rs`
- Test: `lib/rust/src/engine/tests.rs`

**Interfaces:**
- Consumes: `CoreEngine`, `RequestState`, `ControlAction`, `ControlQuery`, `ControlResult`, `windows::Window` de Task 1.
- Produces:
  - `lib/rust/src/config.rs`:
    ```rust
    #[derive(Clone, Copy, PartialEq, Eq, Default)]
    pub enum SelectionMode { #[default] Automatic, ExplicitProvider(&'static str) }

    #[derive(Clone, Default)]
    pub struct SessionConfig {
        pub mode: SelectionMode,
        values: HashMap<String, String>,
    }
    ```
    con métodos `with_automatic()`, `with_explicit_provider(id: &'static str)`, `set_option(key: &str, value: &str) -> Result<()>` y `option(&self, key: &str) -> Option<&str>` (clave `"java.home"` usada por JAB).
  - `lib/rust/src/engine/providers.rs`:
    ```rust
    pub(super) type ProviderId = &'static str;

    pub(super) trait WindowsAccessibilityProvider {
        fn id(&self) -> ProviderId;
        fn prepare_capture(&mut self, request: Arc<RequestState>) -> Result<Option<CapturePreparation>>;
        fn matches_window_async(&mut self, request: Arc<RequestState>, window: windows::Window, retry_until: Option<Instant>) -> Result<Option<Receiver<Result<bool>>>>;
        fn is_retry_candidate(&self, window: &windows::Window) -> bool;
        fn act(&mut self, request: Arc<RequestState>, id: u64, window: windows::Window, action: ControlAction, query: ControlQuery) -> Result<Receiver<Result<Option<ControlResult>>>>;
        fn release(&mut self, id: u64);
        fn retain_sessions(&mut self, sessions: &HashSet<u64>);
        fn check_health(&self);
    }
    ```
    y `ProviderRegistry` que administra una colección ordenada de proveedores + proveedor fallback (`"uia"`).

- [ ] **Step 1: Escribir tests unitarios para `SessionConfig` y `ProviderRegistry`**

Agregar tests en `lib/rust/src/engine/tests.rs` (y/o módulo de pruebas en `providers.rs`):
1. `session_config_validates_non_empty_and_nul_free_options`: verifica inserción/lectura de claves genéricas (`"java.home"`, etc.) y rechazo de valores inválidos con código `"CONFIGURATION"`.
2. `provider_registry_selects_first_matching_provider_and_falls_back_on_error`: prueba con proveedores falsos (`FakeWindowProvider`) que un error del primer proveedor en modo automático recurre al fallback sin abortar la acción, y que en modo explícito selecciona solo al proveedor solicitado.

- [ ] **Step 2: Ejecutar `cargo test` para verificar el fallo inicial**

Run:
```powershell
& "$env:CARGO_HOME/bin/cargo.exe" test --manifest-path lib/rust/Cargo.toml
```
Expected: FAIL porque aún no existen `SessionConfig` ni `ProviderRegistry`.

- [ ] **Step 3: Implementar `SessionConfig` en `config.rs`, actualizar `backend.rs`/`bindings_py.rs` y crear `ProviderRegistry` en `providers.rs`**

1. Crear `lib/rust/src/config.rs` con `SessionConfig` y `SelectionMode`.
2. Reemplazar `SessionOptions` en `engine.rs`, `backend.rs`, `windows_platform.rs` por `SessionConfig`:
   - En `bindings_py.rs`, mantener `Engine::open_session(java_home: &str)` validando ruta absoluta no vacía con `"java_home must be an absolute runtime directory."` (preserva contrato de `test_native.py`) y poblando `config.set_option("java.home", java_home)` + `SelectionMode::ExplicitProvider("jab")`.
   - En `Engine::open_auto_session(java_home: Option<&str>, explicit: bool)`, usar `SelectionMode::Automatic` y guardar `"java.home"` cuando `java_home.is_some()`.
3. Crear `lib/rust/src/engine/providers.rs` definiendo `trait WindowsAccessibilityProvider`, `JavaProviderAdapter` (sobre `Lane<Java>`), `UiaProviderAdapter` (sobre `Lane<Uia>`) y `ProviderRegistry`.
4. Refactorizar `WindowsPlatform` en `lib/rust/src/engine/windows_platform.rs` para usar `ProviderRegistry` sin enumeraciones cerradas `match WindowsProvider`, manteniendo los tests existentes de aislamiento de hilos en `windows_platform::tests`.

- [ ] **Step 4: Ejecutar `cargo test` para verificar que pasan todos los tests de proveedores y configuración**

Run:
```powershell
& "$env:CARGO_HOME/bin/cargo.exe" test --manifest-path lib/rust/Cargo.toml
```
Expected: PASS.

- [ ] **Step 5: Commit de abstracción de configuración y proveedores**

```bash
git add lib/rust/src/
git commit -m "refactor(core): generalizar configuración por proveedor y registro extensible"
```

---

### Task 3: Centralizar `os_language()` en el núcleo Rust (`Platform`) con delegación perezosa en Python

**Files:**
- Modify: `lib/rust/Cargo.toml`
- Modify: `lib/rust/src/windows.rs`
- Modify: `lib/rust/src/engine/platform.rs`
- Modify: `lib/rust/src/engine.rs`
- Modify: `lib/rust/src/bindings_py.rs`
- Modify: `lib/python/playwright_desktop/_native.pyi`
- Modify: `lib/python/playwright_desktop/_backend.py`
- Modify: `lib/python/playwright_desktop/_platform.py`
- Test: `lib/rust/src/engine/tests.rs`, `lib/tests/test_api.py`, `lib/tests/test_native.py`, `lib/tests/test_patch.py`

**Interfaces:**
- Consumes: `bindings_py.rs` y `platform.rs` de Tasks 1–2.
- Produces:
  - En Rust (`windows.rs`): `pub fn host_os_language() -> String` usando `GetUserDefaultUILanguage` + `LCIDToLocaleName` de `Win32_Globalization` (normalizando formato BCP-47, fallback `"en"`).
  - En Rust (`platform.rs` / `engine.rs`): `pub fn host_os_language() -> Result<String>` que delega al adaptador del SO activo (`#[cfg(windows)]` llama a `windows::host_os_language()`, `#[cfg(not(windows))]` devuelve `Failure("UNSUPPORTED", ...)`).
  - En Python (`_native`): `def os_language() -> str`.
  - En Python (`_platform.py`): `os_language() -> str` consulta `_backend.os_language()` (carga perezosa de `_native`), usando fallback `locale.getlocale()` cuando la plataforma no soporta extensión nativa o en entornos unitarios sin binario compilado.

- [ ] **Step 1: Escribir tests de `os_language()` en Rust (`test_native.py` / `tests.rs`) y verificar carga perezosa en Python**

1. En `lib/rust/src/engine/tests.rs` (bajo `#[cfg(windows)]`):
   ```rust
   #[test]
   fn host_os_language_returns_non_empty_bcp47_tag() {
       let tag = crate::windows::host_os_language();
       assert!(!tag.is_empty());
       assert!(!tag.contains('_'));
   }
   ```
2. En `lib/tests/test_native.py`:
   ```python
   def test_os_language_returns_non_empty_bcp47_tag(self):
       tag = self.native.os_language()
       self.assertIsInstance(tag, str)
       self.assertTrue(tag)
       self.assertNotIn("_", tag)
   ```
3. En `lib/tests/test_api.py`: verificar que `playwright_desktop.os_language()` retorna el resultado de `_backend.os_language()` sin usar `ctypes` directamente, y que `test_patch.py::test_install_and_reload_are_idempotent_and_native_import_is_lazy` sigue verificando que `import playwright_desktop` NO carga `_native`.

- [ ] **Step 2: Ejecutar `cargo test` para verificar el fallo inicial**

Run:
```powershell
& "$env:CARGO_HOME/bin/cargo.exe" test --manifest-path lib/rust/Cargo.toml
```
Expected: FAIL porque aún no existe `windows::host_os_language()`.

- [ ] **Step 3: Implementar `host_os_language()` en Rust y delegar desde `_backend.py` / `_platform.py`**

1. En `lib/rust/Cargo.toml`: añadir feature `"Win32_Globalization"` dentro de `[target.'cfg(windows)'.dependencies] windows.features`.
2. En `lib/rust/src/windows.rs`: implementar `pub fn host_os_language() -> String` llamando `GetUserDefaultUILanguage` y `LCIDToLocaleName` de `::windows::Win32::Globalization`, devolviendo `"en"` si falla o queda vacío.
3. Exponer `pub fn host_os_language() -> Result<String>` en `engine/platform.rs` / `engine.rs` y la función PyO3 `#[pyfunction] fn os_language() -> PyResult<String>` en `bindings_py.rs` (`module.add_function(wrap_pyfunction!(os_language, module)?)?`).
4. Actualizar `lib/python/playwright_desktop/_native.pyi` declarando `def os_language() -> str: ...`.
5. En `_backend.py`: agregar `def os_language() -> str` con importación perezosa de `_native` (`load_native()`).
6. En `_platform.py`: reemplazar la llamada directa a `ctypes.windll.kernel32` por delegación en `_backend.os_language()`, capturando `UnsupportedPlatformError` / ausencia de `_native` para conservar el fallback seguro a `locale.getlocale()`.

- [ ] **Step 4: Recompilar wheel y ejecutar `cargo test` + `test_native.py` + `test_api.py` + `test_patch.py`**

Run:
```powershell
& "$env:CARGO_HOME/bin/cargo.exe" test --manifest-path lib/rust/Cargo.toml
.\lib\scripts\build_py.ps1
```
Luego instalar el wheel en carpeta temporal y ejecutar `unittest` sobre `test_native.py`, `test_api.py` y `test_patch.py`.
Expected: PASS 100%, manteniendo `playwright_desktop._native` sin cargar al hacer `import playwright_desktop`.

- [ ] **Step 5: Commit del traslado de `os_language()` al núcleo**

```bash
git add lib/rust/ lib/python/playwright_desktop/ lib/tests/
git commit -m "refactor(core): mover detección de idioma del SO al núcleo nativo"
```

---

### Task 4: Reestructurar `lib/` en Cargo Workspace (`lib/core` puro + `lib/bindings/python` delgado)

**Files:**
- Create: `lib/Cargo.toml`
- Create: `lib/core/Cargo.toml`
- Create: `lib/bindings/python/Cargo.toml`
- Move: `lib/rust/Cargo.lock` -> `lib/Cargo.lock`
- Move: `lib/rust/src/**` (excepto `bindings_py.rs`) -> `lib/core/src/**`
- Move: `lib/rust/tests/abi_layout.c` -> `lib/core/tests/abi_layout.c`
- Move: `lib/rust/src/bindings_py.rs` -> `lib/bindings/python/src/lib.rs`
- Move: `lib/README.md` -> `lib/python/README.md`
- Move: `lib/tests/**` -> `lib/python/tests/**`
- Remove: `lib/rust/Cargo.toml` y `lib/rust/src/lib.rs` (Python-específico reemplazado por `core/src/lib.rs`)
- Modify: `lib/pyproject.toml`
- Modify: `lib/scripts/build_py.ps1`
- Modify: `.github/workflows/publish-pypi.yml`

**Interfaces:**
- Consumes: módulos puros de Tasks 1–3 (`core`) y wrapper PyO3 (`bindings_py.rs`).
- Produces:
  - `lib/Cargo.toml`:
    ```toml
    [workspace]
    members = ["core", "bindings/python"]
    resolver = "3"
    ```
  - `lib/core/Cargo.toml`: `name = "playwright-desktop-core"`, `edition = "2024"`, `rust-version = "1.85"`, sin `pyo3`, con `[target.'cfg(windows)'.dependencies] windows = "=0.62.2"`.
  - `lib/bindings/python/Cargo.toml`: `name = "playwright-desktop-py"`, `edition = "2024"`, `[lib] name = "_native"`, `crate-type = ["cdylib"]`, `dependencies = { pyo3 = ..., playwright-desktop-core = { path = "../../core" } }`.
  - `lib/pyproject.toml`: `readme = "python/README.md"`, `manifest-path = "bindings/python/Cargo.toml"`.

- [ ] **Step 1: Verificar el estado previo de pruebas antes de mover crates**

Confirmar el punto de partida con `cargo test --manifest-path lib/rust/Cargo.toml --locked`.

- [ ] **Step 2: Crear manifiestos del Cargo Workspace (`lib/Cargo.toml`, `lib/core/Cargo.toml`, `lib/bindings/python/Cargo.toml`)**

1. Crear `lib/Cargo.toml` declarando los dos miembros (`"core"`, `"bindings/python"`).
2. Crear `lib/core/Cargo.toml` sin dependencia `pyo3` (`crate-type = ["rlib"]`) y trasladar la sección `[target.'cfg(windows)'.dependencies]` y `[profile.release]` a nivel raíz/workspace según corresponda para Cargo.
3. Crear `lib/bindings/python/Cargo.toml` con `[lib] name = "_native"`, `crate-type = ["cdylib"]`, dependencia a `pyo3 = { version = "=0.30.1", features = ["abi3-py310"] }` y `playwright-desktop-core = { path = "../../core" }`.
4. Definir `lib/core/src/lib.rs` exportando la API pública del núcleo (`pub use config::*; pub use engine::{CoreEngine, CoreReply, CoreRequest, InvokeParams}; pub use error::{Failure, Result}; pub use platform_api::host_os_language;`, manteniendo privados los detalles internos de Win32/JAB/UIA).

- [ ] **Step 3: Mover fuentes/tests/README con `git mv`, limpiar `lib/rust/` y adaptar `bindings/python/src/lib.rs`**

1. Mover los archivos fuente de `lib/rust/src/` a `lib/core/src/` (y `abi_layout.c` a `lib/core/tests/abi_layout.c`).
2. Mover `lib/rust/src/bindings_py.rs` a `lib/bindings/python/src/lib.rs` importando desde `playwright_desktop_core::...`.
3. Mover `lib/README.md` a `lib/python/README.md` y `lib/tests/**` a `lib/python/tests/**`.
4. Trasladar `lib/rust/Cargo.lock` a `lib/Cargo.lock` y actualizarlo dentro del workspace (`cargo update --workspace --manifest-path lib/Cargo.toml` o `cargo metadata`).

- [ ] **Step 4: Actualizar `lib/pyproject.toml`, `lib/scripts/build_py.ps1` y `.github/workflows/publish-pypi.yml`**

1. `lib/pyproject.toml`: actualizar `readme = "python/README.md"` y `[tool.maturin] manifest-path = "bindings/python/Cargo.toml"`.
2. `lib/scripts/build_py.ps1`:
   - Actualizar rutas de limpieza/target de `'rust/target/python'` a `'target/python'`.
   - Ejecutar `maturin build --release --locked --target-dir target/python --out dist/python` y `maturin sdist --manifest-path bindings/python/Cargo.toml --out dist/python` (verificando previamente que el `.tar.gz` resultante empaquete todo el workspace de `lib/`: `Cargo.toml`, `Cargo.lock`, `core/**`, `bindings/python/**`, `python/README.md` y `python/playwright_desktop/**`).
3. `.github/workflows/publish-pypi.yml`:
   - Actualizar rutas al workspace (`lib/Cargo.toml`, `Swatinem/rust-cache@v2` con `workspaces: lib`, `cargo test --manifest-path lib/Cargo.toml --workspace --locked`) y la ruta de pruebas Python (`$env:PYTHONPATH = "$tmp;lib/python/tests"` y `python -B -m unittest discover -s lib/python/tests -v`).

- [ ] **Step 5: Verificar compilación independiente de `core`, árbol de dependencias, `build_py.ps1` e integridad de `.whl` y `.tar.gz`**

Run:
```powershell
& "$env:CARGO_HOME/bin/cargo.exe" tree --manifest-path lib/Cargo.toml -p playwright-desktop-core
& "$env:CARGO_HOME/bin/cargo.exe" test --manifest-path lib/Cargo.toml --workspace --locked
.\lib\scripts\build_py.ps1
```
Validar además:
- La salida de `cargo tree -p playwright-desktop-core` NO contiene `pyo3`.
- `lib/dist/python/playwright_desktop-*.tar.gz` contiene `Cargo.toml`, `Cargo.lock`, `core/Cargo.toml`, `core/src/lib.rs`, `bindings/python/Cargo.toml`, `bindings/python/src/lib.rs`, `python/README.md` y `python/playwright_desktop/__init__.py`.
- El `.whl` generado incluye la metadata de `python/README.md` y NO incluye archivos de `tests/`.
Expected: PASS en todas las comprobaciones.

- [ ] **Step 6: Commit del Cargo Workspace**

```bash
git add lib/ .github/workflows/publish-pypi.yml
git commit -m "refactor(lib): separar núcleo multi-SO y binding Python en workspace"
```

---

### Task 5: Verificación integral de toda la suite y actualización de documentación técnica

**Files:**
- Modify: `docs/documentacion-tecnica.md`
- Modify: `docs/superpowers/plans/2026-11-21-core-multi-os-multilang-refactor.md` (marcar pasos completados)

**Interfaces:**
- Consumes: workspace y artefactos `.whl` + `.tar.gz` generados en Task 4.
- Produces: documentación técnica alineada a la estructura `lib/core` + `lib/bindings/python` + `lib/python/` (sin cubrir publicación ni credenciales) y verificación completa de las pruebas.

- [ ] **Step 1: Actualizar `docs/documentacion-tecnica.md` reflejando la arquitectura de Cargo Workspace**

Actualizar la sección de responsabilidades y `### Desarrollo de la librería` para documentar:
- `lib/core/` contiene el núcleo Rust independiente del lenguaje cliente y del proveedor de accesibilidad.
- `lib/bindings/python/` contiene el adaptador PyO3 y `lib/python/` agrupa el paquete Python (`playwright_desktop/`), su `README.md` (`lib/python/README.md`) y sus pruebas (`lib/python/tests/`).
- El comando de pruebas Rust pasa a `cargo test --manifest-path lib/Cargo.toml --workspace` y las pruebas Python pasan a `py -m unittest discover -s lib/python/tests -v`.
- No documentar flujos de publicación ni credenciales.

- [ ] **Step 2: Ejecutar suite completa Rust + Python + Swing/JAB + Windows UIA + verificación de demo**

Run:
```powershell
& "$env:CARGO_HOME/bin/cargo.exe" test --manifest-path lib/Cargo.toml --workspace --locked
.\lib\scripts\build_py.ps1
```
Instalar el wheel recién generado en directorio temporal y ejecutar:
```powershell
$env:PLAYWRIGHT_DESKTOP_TEST_JAVA_HOME = '<ruta-jdk-si-disponible>'
$env:PLAYWRIGHT_DESKTOP_TEST_WINDOWS_UIA = '1'
py -B -m unittest discover -s lib/python/tests -v
```
Validar `git grep -nE "Restart Python" -- lib/` (0 coincidencias) y confirmar que la demo `demo/pc-info-automation` no tuvo roturas (`py_compile` / ejecución E2E en sesión interactiva).
Expected: PASS 100% (0 fallos, 0 errores).

- [ ] **Step 3: Marcar checkboxes del plan y comitear cierre de documentación**

```bash
git add docs/documentacion-tecnica.md docs/superpowers/plans/2026-11-21-core-multi-os-multilang-refactor.md
git commit -m "docs: documentar arquitectura modular multi-SO y multi-lenguaje en lib"
```
