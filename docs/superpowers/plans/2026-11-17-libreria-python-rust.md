# playwright-desktop — plan de implementación

> Para ejecutar este plan, leer primero la especificación enlazada y avanzar por entregables verificables. La implementación requiere revisión del diseño. Este documento es un plan; no representa tareas ya realizadas.

**Objetivo.** Extender las clases `Page` oficiales mediante una importación y automatizar el formulario Swing de la demo desde `page.desktop` con un motor Rust.

**Arquitectura.** Fachadas Python sync y async sobre un binding PyO3. Un worker Rust por proceso integra `windows-rs` y Java Access Bridge, con recursos confinados a su hilo, selección estricta y operaciones nativas seriales.

**Tecnologías.** Python 3.10–3.14 convencional, Playwright 1.63–1.65, Maturin, PyO3 0.30.1, `windows` 0.62.2, Rust MSVC 1.85+, OpenJDK 11+ y Windows x64. Versiones propuestas sujetas a la prueba de construcción de la tarea 1.

**Especificación.** [Arquitectura y contrato](../specs/2026-11-17-libreria-python-rust-design.md).

## Restricciones globales

- Implementación distribuible dentro de `lib/`; demo en `demo/informacion-equipo/`.
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

- [ ] Comprobar Rust MSVC y herramientas C++, Python convencional x64, versiones disponibles y presencia del bridge en el runtime de la demo. No instalar ni habilitar componentes silenciosamente.
- [ ] Trasladar los bloques TOML de la especificación y comprobar la compatibilidad de versiones mediante una construcción mínima.
- [ ] Desde `lib/`, ejecutar `py -m pip install --user "maturin>=1.16,<2"`, generar `Cargo.lock` y construir con `py -m maturin build --release --locked --out dist`.
- [ ] Instalar el wheel resultante con `py -m pip install --user <ruta-del-wheel>`. Desde una carpeta fuera del repositorio, importar `_native`, consultar `binding_version()` y confirmar que no se necesita el árbol fuente.
- [ ] Registrar target, Python, toolchain y etiqueta de wheel utilizados. Resolver incompatibilidades antes de continuar; todavía no hay una API desktop funcional.

## Tarea 2 — parche y contrato Python

**Archivos:** crear `_patch.py`, `_models.py`, `_errors.py`, `_sync.py`, `_async.py`, `typing.py`, `py.typed`, `tests/test_patch.py` y `tests/test_api.py`. Modificar `__init__.py`.

**Produce:** `install() -> None`, `uninstall() -> None` y el descriptor `Page.desktop`. Las fachadas comparten validación de consultas y ofrecen los métodos especificados, inicialmente con el binding nativo sustituido por un doble solo en pruebas.

- [ ] Escribir primero pruebas de clase oficial importada antes y después de la extensión, idempotencia, recarga, dos clases sync/async, atributo ajeno y reversión ante un fallo de instalación. Verificar la identidad de `goto`, `click`, `locator` y `close`.
- [ ] Implementar prevalidación transaccional y marcadores de propiedad/versionado de los descriptores bajo lock. No acceder a internals de Playwright.
- [ ] Añadir estado débil por página, configuración por fachada y cierre solicitado desde el evento público `Page.on("close", ...)`.
- [ ] Probar consultas por rol/nombre exactos, rechazo de roles y timeouts inválidos, y ausencia de inicialización nativa durante importación o acceso simple al descriptor.
- [ ] Ejecutar `py -m unittest discover -s tests -p "test_*.py" -v` desde `lib/`. Los tests que requieren escritorio deben estar separados y omitirse con motivo explícito cuando no se habiliten.

## Tarea 3 — acceso a Windows y cliente JAB

**Archivos:** crear `rust/src/windows.rs`, `rust/src/jab/{mod.rs,ffi.rs,loader.rs}`, `rust/src/error.rs` y `rust/tests/abi_layout.c`. Ampliar el binding y los stubs.

**Consume:** ruta absoluta del runtime y filtros de ventana. **Produce:** carga del bridge, información básica de ventanas, lectura del contexto raíz y objetos Java con propiedad y liberación explícitas. Los handles se mantienen dentro de Rust.

- [ ] Escribir pruebas del cargador para ruta ausente, DLL inexistente, símbolos ausentes y arquitectura incompatible, con fallos tipados y sin búsquedas en el directorio de trabajo.
- [ ] Implementar las llamadas mínimas de `windows-rs` y la tabla de símbolos JAB, basándose en las exportaciones de `AccessBridgeCalls.c` de OpenJDK 11.
- [ ] Compilar una sonda C de pruebas contra `jni.h` y los headers JAB del JDK, con las definiciones de arquitectura correspondientes. Comparar tamaños, alineaciones y offsets con las estructuras Rust. La sonda no forma parte del wheel ni añade una dependencia C al motor.
- [ ] Implementar decodificación UTF-16 y objetos RAII; probar textos con tildes, caracteres no BMP, límites de buffer y liberación de objetos en rutas de error.
- [ ] Ejecutar `cargo test --locked` desde `lib/rust/` con Python y headers configurados. En una comprobación Windows separada, cargar el bridge e identificar la raíz de un formulario Java abierto.
- [ ] Documentar el paso explícito de habilitación con `jabswitch -enable` del runtime seleccionado y su diagnóstico. No modificar Windows o Java como efecto de importar el paquete.

## Tarea 4 — selección y operaciones básicas

**Archivos:** ampliar `jab/mod.rs`, `windows.rs` y `error.rs`; crear `rust/src/engine.rs` con los comandos tipados y sus pruebas.

**Contrato nativo:** `open_session(java_home) -> session_id`, `new_request(session_id, timeout_ms) -> request_id`, `prepare_window(session_id, title, pid, request_id) -> capture_id`, `wait_window(capture_id, request_id) -> window_id`, `attach_window(session_id, title, pid, request_id) -> window_id`, `read_text(window_id, role, name, request_id) -> str`, `fill(window_id, role, name, text, request_id) -> None`, `click(window_id, role, name, request_id) -> None`, `wait_closed(window_id, request_id) -> None`, `cancel_request(request_id) -> None` y `close_session(session_id) -> None`. `new_request` registra sin esperar al worker el identificador, el plazo monótono y el estado de cancelación; por eso se puede cancelar incluso antes de que comience `to_thread`. La preparación y espera de una captura comparten el plazo original. El binding puede encapsular estos identificadores en objetos Python internos; no son la API pública del usuario.

- [ ] Probar cero, una y múltiples coincidencias, filtros de PID, controles repetidos y límites del recorrido. Un recorrido truncado debe fallar aunque solo haya encontrado un candidato hasta ese punto.
- [ ] Implementar recorrido iterativo y acotado, normalización de rol/estado desde variantes inglesas y resolución de consultas dentro de la ventana vinculada.
- [ ] Probar que un HWND o un contexto raíz obsoleto no se reasignan silenciosamente a otra ventana.
- [ ] Implementar `fill` con `setTextContents` y lectura de comprobación; rechazar texto demasiado largo o campos no editables antes de escribir.
- [ ] Implementar el clic mediante la acción accesible del botón, invocada una sola vez. No reintentar efectos ante una respuesta desconocida.
- [ ] Implementar las transiciones de reserva definidas en la especificación. Probar que salir correctamente de `expect_window()` conserva la reserva y que cerrar ventana, página o sesión la libera.
- [ ] Ejecutar las pruebas Rust y una prueba Windows de lectura, escritura y botón sobre el formulario de la demo. Registrar esa comprobación como desktop parcial hasta completar ClickOnce.

## Tarea 5 — worker, integración sync/async y recuperación

**Archivos:** completar `engine.rs`, `lib.rs`, `_sync.py`, `_async.py`, `_native.pyi`, `_errors.py` y `tests/test_async.py`.

**Produce:** un motor compartido por proceso con ruta de bridge inmutable después del arranque, mensajes Windows, plazos monótonos y errores traducidos a Python. La fachada pública implementa el mismo contrato en sync y async.

- [ ] Probar que la DLL y los objetos JAB solo se utilizan desde su hilo propietario. Una segunda ruta de runtime después de inicializar el motor debe generar error de configuración.
- [ ] Implementar la cola de comandos y el loop de mensajes. Las capturas pendientes avanzan por ciclos sin retener toda la cola durante el timeout.
- [ ] Utilizar `Python::detach` al esperar resultados. Probar que un heartbeat del event loop sigue avanzando durante una espera nativa async.
- [ ] Conectar `asyncio.to_thread` solo a bindings Rust. Verificar que los callbacks y métodos Playwright siguen ejecutándose en el hilo/event loop original.
- [ ] Definir identificadores de solicitud antes de lanzar el trabajo async, de modo que cancelarlo no dependa de que el worker devuelva un resultado. Probar cancelación antes de encolar, en cola y durante una llamada ya iniciada.
- [ ] Probar plazos que incluyan el tiempo de cola, timeout cero, cierre de página, recolección y cierre explícito. Las solicitudes canceladas no se ejecutan después de vencer si aún estaban pendientes.
- [ ] Simular una llamada JAB bloqueada en un doble controlable, marcar el motor no utilizable al superar su plazo y rechazar solicitudes posteriores hasta reiniciar Python. Liberar el doble al terminar para que la prueba no deje un hilo bloqueado.
- [ ] Probar traducción de errores y captura del timeout mediante el tipo público de Playwright, conservando la causa nativa.
- [ ] Reconstruir el wheel y ejecutar las pruebas Python y Rust contra la instalación resultante.

## Tarea 6 — recorrido real de la demo

**Archivos:** crear `lib/tests/windows/test_java.py` y `test_demo.py`. Actualizar las instrucciones de la demo únicamente cuando lo requiera la preparación explícita de JAB.

**Consume:** demo compilada, OpenJDK con JAB activo, Edge con activación ClickOnce disponible y wheel instalado. **Produce:** evidencia del recorrido que utiliza la clase oficial `Page` con una sola importación adicional.

- [ ] Primero repetir manualmente la activación de la publicación corregida con `/nowin32manifest`. Resolver cualquier bloqueo real de ClickOnce antes de atribuirlo al motor desktop.
- [ ] Ejecutar `demo/informacion-equipo/start.ps1` en una terminal controlada y usar su dirección efectiva, por defecto `127.0.0.1:18765`. Documentar cómo se detiene el proceso al finalizar la prueba.
- [ ] Probar una página creada antes de importar la extensión y otra creada después. Mantener los imports oficiales de Playwright.
- [ ] Registrar `expect_window()` antes del clic web, esperar Java, editar los tres campos con datos de prueba y pulsar **Confirmar** mediante la API accesible.
- [ ] Comprobar cierre de Java y valores en la web; obtener el PDF mediante la API pública de solicitudes de Playwright y verificar sus tres valores con pypdf. Inspeccionar también la vista previa del PDF.
- [ ] Repetir el recorrido con `playwright.async_api` usando `async with` y `await`, con un heartbeat que compruebe que la espera nativa no bloquea el event loop.
- [ ] Comprobar una ventana existente, dos ventanas indistinguibles, bridge deshabilitado y cierre prematuro de Java. Las pruebas que alteren la configuración JAB son opt-in y deben restaurar su estado anterior.
- [ ] Ejecutar las pruebas nativas en serie. La captura de ventanas no debe atribuir automáticamente ventanas de otros procesos a una página.
- [ ] Desde `lib/`, ejecutar las pruebas Windows de forma explícita con `py -m unittest discover -s tests/windows -p "test_*.py" -v`, en la sesión interactiva preparada. Un prerrequisito ausente debe informar qué falta, sin registrar la prueba como completada.

## Tarea 7 — distribución, documentación y medidas

**Archivos:** actualizar `lib/README.md`, README principal, documentación técnica y `.gitignore`; añadir `lib/benchmarks/measure_java.py` solo para medir las operaciones implementadas.

- [ ] Verificar instalación del wheel fuera del repositorio, en CPython 3.10 y 3.14 convencionales x64 y Playwright 1.63.0 y 1.65.2. Ampliar versiones declaradas solo después de comprobarlas.
- [ ] Medir en release inicialización, búsquedas, lecturas, ediciones y acciones; guardar p50/p95, número de muestras y condiciones de ejecución. Separar ClickOnce, JVM, motor y PDF.
- [ ] Repetir operaciones y observar liberación de sesiones, objetos JAB e hilos. Investigar crecimiento sostenido antes de declarar estable la versión.
- [ ] Documentar instalación con Python global, configuración del runtime, API sync/async, errores, limitación de sesión serial y alcance del tipado estático.
- [ ] Publicar el estado real de verificación; mantener pendientes explícitos si algún recorrido no se ha ejecutado.
- [ ] Ejecutar revisión independiente del cambio completo, corregir problemas de ABI, propiedad de recursos, colisiones y efectos tardíos, y repetir únicamente las comprobaciones afectadas.

## Resultado esperado

Una librería pequeña que añade `page.desktop` al importar y completa el caso Java de la demo utilizando interfaces de accesibilidad reales. El criterio de entrega incluye wheel instalable y recorridos sync y async comprobados; compilar el crate o manipular directamente la API HTTP de la demo no es suficiente.
