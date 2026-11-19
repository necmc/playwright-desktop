# playwright-desktop — plan de implementación

**Objetivo.** Extender las clases `Page` oficiales mediante una importación y automatizar el formulario Swing de la demo desde `page.desktop` con un motor Rust.

**Arquitectura.** Fachadas Python sync y async sobre un binding PyO3. Un worker Rust por proceso integra `windows-rs` y Java Access Bridge, con recursos confinados a su hilo, selección estricta y operaciones nativas seriales.

**Tecnologías.** Python 3.10–3.14 convencional, Playwright 1.63–1.65, Maturin, PyO3 0.30.1, `windows` 0.62.2, Rust MSVC 1.85+, OpenJDK 11+ y Windows x64. La tarea 7 registra la validación de las versiones declaradas.

**Especificación.** [Arquitectura y contrato](../specs/2026-11-17-libreria-python-rust-design.md).

**Estado.** La base Python/Rust está implementada y la versión `0.1.0` está publicada. Las tareas 1 a 3 están cerradas. El siguiente corte corresponde a la selección de ventanas y controles en la tarea 4.

Marca cada paso al cumplir su criterio de cierre y registra allí la evidencia. Si una comprobación se registra en otro plan, enlaza su tarea.

## Restricciones globales

La automatización de la demo utiliza **Page Object Model (POM)** y un único escenario funcional. El test contiene el recorrido, sus aserciones, la importación de `playwright_desktop` y la llamada a `page.desktop.expect_window()`. El Page Object Java recibe la ventana capturada mediante `opened.value` y encapsula los locators y las acciones. `fixtures/` prepara el navegador y el runtime, `config.py` contiene la configuración y `utils/` extrae el texto del PDF. Mantener unittest, sin nuevas dependencias ni capas genéricas de acciones. Las pruebas del motor pertenecen a la librería.

El runner comprueba que la demo ya esté en ejecución y utiliza un perfil temporal de navegador, salvo que se haya preparado uno para extensiones. Los comandos de la demo evitan escribir caché de bytecode Python. El runner no genera capturas, JSON de descargas ni logs Java automáticamente.

- Implementación distribuible dentro de `lib/`; demo en `demo/pc-info/` y toda su automatización mediante la librería en `demo/pc-info-automation/`.
- El usuario conserva imports, fixtures y métodos web oficiales. Solo añade `import playwright_desktop`.
- Añadir exclusivamente `Page.desktop`, tanto a la clase pública sync como a la async.
- Carga del motor diferida, configuración JAB explícita y sin cambios automáticos de accesibilidad al importar.
- Un motor JAB por proceso y un flujo desktop activo; ejecución nativa serial en el escritorio compartido.
- Sin OCR, coordenadas, control de instaladores, backend UIA o servicios externos en esta versión.
- Sin dependencias de automatización adicionales. Usar bibliotecas estándar para concurrencia y pruebas unitarias.
- Compilar e instalar con Python global. La ejecución de un wheel precompilado no requiere el toolchain Rust.
- Las pruebas nativas requieren una sesión Windows interactiva. Las verificaciones sin JVM no acreditan el recorrido real.

## Foco de revisión

| Riesgo | Comportamiento requerido | Tarea |
| --- | --- | --- |
| Colisión de `Page.desktop` o recarga del paquete | Prevalidación de ambas clases, idempotencia y ausencia de parche parcial. | 2 |
| DLL, bitness o layout JAB incorrectos | Diagnóstico antes de actuar y ABI contrastada con headers reales. | 3 |
| Dos ventanas similares, controles repetidos o HWND reutilizado | Error explícito; ninguna acción sobre una coincidencia arbitraria. | 4 |
| Cancelación, cierre de `Page` o JVM sin respuesta | Cancelar comandos pendientes, liberar reservas y rechazar un motor bloqueado. | 5 |
| Demo funciona por API pero no desde el navegador | Verificación real de ClickOnce, Java, retorno web y PDF con valores editados. | 6 |

## Tarea 1 — paquete mixto y primer wheel

**Archivos:** crear `lib/pyproject.toml`, `lib/rust/Cargo.toml`, `lib/rust/Cargo.lock`, `lib/rust/src/lib.rs`, `lib/python/playwright_desktop/__init__.py` y `_native.pyi`. Actualizar `.gitignore` para `lib/rust/target/`, `lib/dist/` y metadatos de construcción.

**Contrato:** el módulo `playwright_desktop._native` exporta `binding_version() -> str`; importar el paquete no crea hilos nativos ni carga JAB. El nombre de módulo coincide en Maturin, Cargo y `#[pymodule]`.

- [x] Configurar el paquete mixto y construir un wheel Windows x64 con el `Cargo.lock` del proyecto. Entorno verificado con Python 3.14.4, Rust 1.100.0 y Maturin 1.16.0. El comando actual es `lib/scripts/build_py.ps1`.
- [x] Instalar el wheel en Python global e importar el módulo nativo fuera del repositorio. Playwright 1.63.0 fue la versión instalada para la comprobación.

## Tarea 2 — parche y contrato Python

**Archivos:** crear `_patch.py`, `_models.py`, `_errors.py`, `_sync.py`, `_async.py`, `typing.py`, `py.typed`, `tests/test_patch.py` y `tests/test_api.py`. Modificar `__init__.py`.

**Produce:** `install() -> None`, `uninstall() -> None` y el descriptor `Page.desktop`. Las fachadas comparten validación de consultas y ofrecen los métodos especificados, inicialmente con el binding nativo sustituido por un doble solo en pruebas.

- [x] Implementar y probar el parche transaccional sobre las clases oficiales sync y async: idempotencia, recarga, carga diferida, colisión, reversión y conservación de las acciones web.
- [x] Añadir estado débil por página, configuración por fachada y cierre solicitado desde el evento público `Page.on("close", ...)`. Las pruebas cubren referencias débiles y validaciones de títulos, PID, roles, nombres, plazos y texto.
- [x] Asociar la captura exitosa a su página. En sync y async, `page.desktop.get_by_role()` y `wait_for_closed()` usan la ventana capturada. El contexto conserva `.value` para las referencias explícitas.

## Tarea 3 — acceso a Windows y cliente JAB

**Archivos.** `lib/rust/src/windows.rs`, `lib/rust/src/jab/{mod.rs,ffi.rs,loader.rs,tests.rs}`, `lib/rust/src/error.rs`, `lib/rust/tests/abi_layout.c`, `lib/tests/test_api.py` y `lib/tests/test_native.py`.

**Consume:** ruta absoluta del runtime y filtros de ventana. **Produce:** carga del bridge, información básica de ventanas, lectura del contexto raíz y objetos Java con propiedad y liberación explícitas. Los handles se mantienen dentro de Rust.

- [x] Implementar las llamadas de `windows-rs`, la carga JAB desde el runtime seleccionado, la decodificación UTF-16 y la liberación con RAII. Los diagnósticos de bridge ausente y arquitectura incorrecta tienen cobertura automatizada.
- [x] Contrastar tamaños, alineaciones y offsets del ABI con una sonda C compilada con los headers del JDK. La sonda queda fuera del wheel.
- [x] Cargar el bridge e identificar la raíz de un formulario real en Windows. El orquestador de la demo permite habilitarlo mediante `-EnableAccessBridge`.
- [x] Comprobar los diagnósticos de ruta Java sin configurar, DLL ausente, arquitectura x86 y símbolo `Windows_run` faltante.
- [x] Rechazar una lectura si el rango UTF-16 termina antes de la longitud anunciada o carece de terminador. Las pruebas verifican la liberación del candidato y de los hijos pendientes si falla la decodificación del árbol.
- [x] Verificar Swing con JDK 11.0.21. La ventana visible agotó la espera con Java Access Bridge deshabilitado en un perfil temporal. Al habilitar el bridge en el proceso JVM, se leyó y editó el mismo control con 1.023 unidades UTF-16, incluido un carácter suplementario.

**Verificación del corte.** Pasaron 8 pruebas Rust y 24 pruebas Python contra un wheel Windows x64 recién compilado.

## Tarea 4 — selección y operaciones básicas

**Archivos:** ampliar `jab/mod.rs`, `windows.rs` y `error.rs`; crear `rust/src/engine.rs` con los comandos tipados y sus pruebas.

**Contrato nativo:** `open_session(java_home) -> session_id`, `new_request(session_id, timeout_ms) -> request_id`, `prepare_window(session_id, title, pid, request_id) -> capture_id`, `wait_window(capture_id, request_id) -> window_id`, `attach_window(session_id, title, pid, request_id) -> window_id`, `read_text(window_id, role, name, request_id) -> str`, `fill(window_id, role, name, text, request_id) -> None`, `click(window_id, role, name, request_id) -> None`, `wait_closed(window_id, request_id) -> None`, `cancel_request(request_id) -> None` y `close_session(session_id) -> None`. `new_request` registra sin esperar al worker el identificador, el plazo monótono y el estado de cancelación; por eso se puede cancelar incluso antes de que comience `to_thread`. La preparación y espera de una captura comparten el plazo original. El binding puede encapsular estos identificadores en objetos Python internos; no son la API pública del usuario.

- [x] Implementar el recorrido iterativo con límites, la normalización de rol y estado, `fill` con lectura de comprobación y el clic mediante una única acción accesible.
- [x] Implementar reserva y vinculación de la ventana. La comprobación con Java iniciado de forma directa verificó captura, lectura, edición, confirmación y cierre mediante `page.desktop`.
- [ ] Probar cero, una y múltiples coincidencias, filtros de PID, controles repetidos y límites del recorrido. Un recorrido truncado debe fallar aunque solo haya encontrado un candidato hasta ese punto.
- [ ] Comprobar que terminar correctamente `expect_window()` mantiene la reserva hasta confirmar el cierre de ventana, página o sesión. Cubrir HWND reutilizado junto con la regresión de raíz obsoleta en la [tarea 5](#tarea-5--worker-integración-syncasync-y-recuperación).

## Tarea 5 — worker, integración sync/async y recuperación

**Archivos:** completar `engine.rs`, `lib.rs`, `_sync.py`, `_async.py`, `_native.pyi`, `_errors.py` y `tests/test_async.py`.

**Produce:** un motor compartido por proceso con ruta de bridge inmutable después del arranque, mensajes Windows, plazos monótonos y errores traducidos a Python. La fachada pública implementa el mismo contrato en sync y async.

- [x] Implementar el worker con cola y mensajes Windows, `Python::detach`, `asyncio.to_thread` para las llamadas nativas y solicitudes cancelables antes de encolar. Las pruebas cubren plazos monótonos, cancelación previa y durante una operación nativa, respuesta del bucle durante una espera y cierre de sesión.
- [x] Traducir los errores nativos a Python y comprobar que el timeout se puede capturar mediante el tipo público de Playwright.
- [x] Ejecutar 19 pruebas Python contra el wheel instalado y 4 pruebas Rust en el corte de la base inicial.
- [ ] Cubrir las regresiones de ciclo de vida en `lib/tests/` y las pruebas Rust. **Cierre.** Una petición corta en cola puede vencer sin invalidar una petición activa con plazo vigente. Un inicio nativo lento mantiene activo el bucle async. Cancelar la preparación libera la reserva incluso si ya se encoló. Una raíz Java obsoleta libera su ventana.
- [ ] Verificar la afinidad al hilo propietario de JAB, el rechazo de un segundo runtime y que las llamadas y callbacks Playwright permanecen en su bucle original.
- [ ] Verificar timeout cero, cierre de página, cierre prematuro de ventana, recolección y cierre explícito con solicitudes pendientes. Confirmar que las solicitudes canceladas o vencidas en cola se descartan.
- [ ] Simular una llamada JAB bloqueada en un doble controlable, marcar el motor no utilizable al superar su plazo y rechazar solicitudes posteriores hasta reiniciar Python. Liberar el doble al terminar para que la prueba no deje un hilo bloqueado.
- [ ] Tras las regresiones, reconstruir el wheel y repetir las pruebas Python contra la instalación resultante y las pruebas Rust del crate.

## Tarea 6 — recorrido real de la demo

**Archivos:** `demo/pc-info-automation/run.py` y `tests/test_flow.py`, con Page Objects en `pages/` y preparación en `fixtures/`. Instrucciones en `demo/README.md`, único README de las demos. Los casos técnicos del motor se verifican como parte de la librería.

**Consume:** demo compilada, OpenJDK con JAB activo, Edge con activación ClickOnce disponible y wheel instalado. **Produce:** evidencia del recorrido que utiliza la clase oficial `Page` con una sola importación adicional.

Ejecutar en serie las verificaciones de escritorio. La captura debe distinguir las ventanas de la operación mediante los filtros y criterios de la [tarea 4](#tarea-4--selección-y-operaciones-básicas).

- [x] Organizar un único escenario funcional sync con POM, configuración y fixtures. Se ejecuta con `py demo/pc-info-automation/run.py` y la demo ya levantada. Las comprobaciones técnicas del motor pertenecen a `lib/`.
- [x] Verificar por separado la web y el contenido textual del PDF en Edge headless, simulando la confirmación HTTP. Con Java iniciado directamente también pasaron la captura, la edición, el cierre y las comprobaciones de la web y del PDF.
- [ ] Diagnosticar la captura de Java tras ClickOnce. Registrar título, tiempos y errores del lanzador y del proceso Java. **Cierre.** Con la [activación desde Edge](2026-11-17-demo-clickonce-java.md#tarea-3--lanzador-y-publicación) comprobada, `expect_window()` captura la ventana de la operación y la limpieza se limita a los procesos del flujo. El 19 de noviembre de 2026, el escenario PC-01 agotó la espera de Java al solicitar la apertura desde Edge.
- [ ] Completar el recorrido sync PC-01 con el [plan de pruebas](../../../demo/pc-info-automation/test_plan.md). **Cierre.** Desde el botón web, editar los tres campos en Java, confirmar, verificar el cierre y los datos de la web, comprobar el texto del PDF y revisar visualmente la vista previa en el navegador.
- [ ] Completar la comprobación async del mismo recorrido. Las regresiones de cancelación y respuesta del bucle se cubren en la [tarea 5](#tarea-5--worker-integración-syncasync-y-recuperación). Mantener un único escenario en la automatización de ejemplo.
- [ ] Verificar una página creada antes y otra después de importar la extensión.

## Tarea 7 — distribución, documentación y medidas

**Archivos:** `lib/scripts/`, `lib/README.md`, README principal y documentación técnica.

- [x] Confirmar `0.1.0` en PyPI. La compilación continúa con Maturin y el script de publicación usa `twine upload`. La migración del script quedó posterior a la publicación de `0.1.0`.
- [ ] Completar la matriz del wheel fuera del repositorio. **Cierre.** Construcción e importación verificadas con CPython 3.10 convencional x64 y comprobación con Playwright 1.65.2. Usar como referencia CPython 3.14 y Playwright 1.63.0 ya verificados. Ajustar los rangos declarados a la evidencia.
- [ ] Medir en release inicialización, búsquedas, lecturas, ediciones y acciones. **Cierre.** Registrar p50/p95, número de muestras, condiciones reproducibles y liberación de sesiones, objetos JAB e hilos tras operaciones repetidas. Medir por separado ClickOnce, JVM, motor y PDF.
- [ ] Revisar el contrato y preparar la entrega. **Cierre.** Instalación, API sync/async, configuración del runtime, tipado, errores y límites de sesión coinciden con las comprobaciones registradas. Hacer una revisión independiente de ABI, propiedad de recursos, colisiones y efectos tardíos y repetir las comprobaciones afectadas.

## Resultado esperado

Una librería pequeña que añade `page.desktop` al importar y completa el caso Java de la demo utilizando interfaces de accesibilidad reales. El criterio de entrega incluye wheel instalable y recorridos sync y async comprobados; compilar el crate o manipular directamente la API HTTP de la demo no es suficiente.
