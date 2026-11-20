# playwright-desktop — plan de implementación

**Objetivo.** Extender las clases `Page` oficiales mediante una importación y automatizar controles de escritorio de Windows desde `page.desktop`.

**Arquitectura.** Fachadas Python sync y async sobre un binding PyO3. El coordinador Rust gestiona las sesiones y delega el acceso al adaptador de Windows. Java Access Bridge (JAB) y UI Automation (UIA) conservan sus recursos en hilos independientes.

**Tecnologías.** Python 3.10–3.14 convencional, Playwright 1.63–1.65, Maturin, PyO3 0.30.1, `windows` 0.62.2, Rust MSVC 1.85+, OpenJDK 11+ y Windows x64. La tarea 7 registra la validación de las versiones declaradas.

**Especificación.** [Arquitectura y contrato](../specs/2026-11-17-libreria-python-rust-design.md).

**Estado.** En validación. Las tareas 1 a 5, el [plan de Windows nativo](2026-11-19-windows-nativo-uia.md) y el recorrido `CP-PI-01` sync con `has_text` están cerrados. Quedan por verificar el recorrido async, la compatibilidad Python 3.10, el rendimiento y la revisión final.

Marca cada paso al cumplir su criterio de cierre y registra allí la evidencia. Si una comprobación se registra en otro plan, enlaza su tarea.

## Restricciones globales

La automatización de la demo utiliza **Page Object Model (POM)** y unittest. CP-PI-01 captura la ventana Java con `page.desktop.expect_window()` y valida los datos y el PDF. CP-PI-02 acciona la apertura desde la interfaz del navegador. Las pruebas del motor pertenecen a `lib/`.

- Implementación distribuible dentro de `lib/`; demo en `demo/pc-info/` y toda su automatización mediante la librería en `demo/pc-info-automation/`.
- El usuario conserva imports, fixtures y métodos web oficiales. Solo añade `import playwright_desktop`.
- Añadir exclusivamente `Page.desktop`, tanto a la clase pública sync como a la async.
- Carga del motor diferida y selección del proveedor por ventana. JAB toma la configuración de Java cuando aplica.
- Un flujo desktop activo por proceso de pruebas, con operaciones de cada proveedor en su hilo.
- Sin OCR, coordenadas, control de instaladores ni servicios externos.
- Sin dependencias de automatización adicionales. Usar bibliotecas estándar para concurrencia y pruebas unitarias.
- Compilar e instalar con Python global. La ejecución de un wheel precompilado no requiere el toolchain Rust.
- Las pruebas nativas requieren una sesión Windows interactiva. Las verificaciones sin JVM no acreditan el recorrido real.

## Foco de revisión

| Riesgo | Comportamiento requerido | Tarea |
| --- | --- | --- |
| Colisión de `Page.desktop` o recarga del paquete | Prevalidación de ambas clases, idempotencia y ausencia de parche parcial. | 2 |
| DLL, bitness o layout JAB incorrectos | Diagnóstico antes de actuar y ABI contrastada con headers reales. | 3 |
| Dos ventanas similares, controles repetidos o HWND reutilizado | Error explícito; ninguna acción sobre una coincidencia arbitraria. | 4 |
| Cancelación, cierre de `Page` o JVM sin respuesta | Cancelar comandos pendientes, liberar reservas y aislar un proveedor bloqueado. | 5 |
| Demo funciona por API pero no desde el navegador | Verificación real de ClickOnce, Java, retorno web y PDF con valores editados. | 6 |

## Tarea 1 — paquete mixto y primer wheel

**Archivos:** crear `lib/pyproject.toml`, `lib/rust/Cargo.toml`, `lib/rust/Cargo.lock`, `lib/rust/src/lib.rs`, `lib/python/playwright_desktop/__init__.py` y `_native.pyi`. Actualizar `.gitignore` para `lib/rust/target/`, `lib/dist/` y metadatos de construcción.

**Contrato:** el módulo `playwright_desktop._native` exporta `binding_version() -> str`; importar el paquete no crea hilos nativos ni carga JAB. El nombre de módulo coincide en Maturin, Cargo y `#[pymodule]`.

- [x] Configurar el paquete mixto y construir un wheel Windows x64 con `lib/scripts/build_py.ps1` y el `Cargo.lock` del proyecto.
- [x] Instalar el wheel e importar el módulo nativo fuera del árbol de fuentes.

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
- [x] Cargar el bridge e identificar la raíz de un formulario real en Windows. El lanzador activa el puente al iniciar Java.
- [x] Comprobar los diagnósticos de ruta Java sin configurar, DLL ausente, arquitectura x86 y símbolo `Windows_run` faltante.
- [x] Rechazar una lectura si el rango UTF-16 termina antes de la longitud anunciada o carece de terminador. Las pruebas verifican la liberación del candidato y de los hijos pendientes si falla la decodificación del árbol.
- [x] Leer y editar un control Swing con 1.023 unidades UTF-16, incluido un carácter suplementario, con el bridge habilitado en la JVM.

## Tarea 4 — selección y operaciones básicas

**Archivos.** `lib/rust/src/engine.rs`, `lib/rust/src/jab/{mod.rs,loader.rs,tests.rs}`, `lib/tests/test_selection.py` y `lib/tests/fixtures/SelectionForm.java`.

**Contrato nativo.** El binding abre una sesión con `open_auto_session()`, crea solicitudes cancelables con `new_request()` y despacha los comandos mediante `invoke()`. La preparación y espera de una captura comparten el plazo original. [_native.pyi](../../../lib/python/playwright_desktop/_native.pyi)

- [x] Implementar el recorrido iterativo con límites, la normalización de rol y estado, `fill` con lectura de comprobación y el clic mediante una única acción accesible.
- [x] Implementar reserva y vinculación de la ventana. La comprobación con Java iniciado de forma directa verificó captura, lectura, edición, confirmación y cierre mediante `page.desktop`.
- [x] Verificar títulos sin coincidencia, selección por PID y rechazo de ventanas o controles duplicados. `expect_window()` distingue una ventana recién abierta de otra con el mismo título que ya existía.
- [x] Rechazar un árbol que supere 4.096 nodos o 64 niveles de profundidad antes de pedir referencias fuera del límite, incluso si ya hay un candidato.
- [x] Verificar que `expect_window()` conserva la reserva tras la captura y que se libera cuando se cierra la ventana, llega el evento de cierre de la página o termina la sesión. Una prueba con un HWND real y una raíz Java simulada verifica que el motor rechaza una acción si cambia la raíz asociada.

Para repetir las pruebas Swing en una sesión interactiva de Windows, indica el JDK en `PLAYWRIGHT_DESKTOP_TEST_JAVA_HOME` y ejecuta `py -m unittest discover -s lib/tests -p test_selection.py -v`.

## Tarea 5 — worker, integración sync/async y recuperación

**Archivos.** Motor en `lib/rust/src/engine.rs`, pruebas del motor en `lib/rust/src/engine/tests.rs` y pruebas Python en `lib/tests/`.

**Produce:** un motor compartido por proceso con ruta de bridge inmutable después del arranque, mensajes Windows, plazos monótonos y errores traducidos a Python. La fachada pública implementa el mismo contrato en sync y async.

- [x] Implementar el worker con cola y mensajes Windows, `Python::detach`, `asyncio.to_thread` para las llamadas nativas y solicitudes cancelables antes de encolar. Las pruebas cubren plazos monótonos, cancelación previa y durante una operación nativa, respuesta del bucle durante una espera y cierre de sesión.
- [x] Traducir los errores nativos a Python y comprobar que el timeout se puede capturar mediante el tipo público de Playwright.
- [x] Cubrir las regresiones de ciclo de vida en `lib/tests/` y Rust. Una petición corta en cola puede vencer sin invalidar una petición activa con plazo vigente. El bucle async responde durante un inicio nativo lento. Una preparación cancelada libera la reserva antes de la siguiente solicitud, incluso si ya la había tomado. Una raíz Java obsoleta libera su ventana.
- [x] Verificar que el worker consulta y libera las referencias JAB en su hilo, que se rechaza un segundo runtime y que el acceso a una página real de Playwright y su evento de cierre ocurren en el bucle original. La prueba de la página usó un motor de escritorio simulado.
- [x] Verificar timeout cero, cierre de página, cierre externo de ventana, recolección y cierre explícito con solicitudes pendientes. Las pruebas del worker confirman que los comandos cancelados, vencidos o de sesiones cerradas se descartan antes de actuar.
- [x] Simular una llamada bloqueada con un doble controlable. El proveedor queda fuera de servicio y otro proveedor sigue atendiendo. La prueba libera el doble y verifica que el proveedor bloqueado siga aislado.
- [x] Verificar la importación del wheel Windows x64 y ejecutar las suites Python y Rust, incluidas las pruebas Swing.

## Tarea 6 — recorrido real de la demo

**Archivos:** `demo/pc-info-automation/run.py` y `tests/test_flow.py`, con Page Objects en `pages/` y preparación en `fixtures/`. Instrucciones en `demo/README.md`, único README de las demos. Los casos técnicos del motor se verifican como parte de la librería.

La validación usa la demo compilada, Microsoft Edge con ClickOnce habilitado y el wheel local. Registra la integración de `page.desktop` con la clase oficial `Page`.

Ejecutar en serie las verificaciones de escritorio. La captura debe distinguir las ventanas de la operación mediante los filtros y criterios de la [tarea 4 de la librería](#tarea-4--selección-y-operaciones-básicas). El cierre requiere completar CP-PI-01 en sync, cuya evidencia se registra en el [plan de activación ClickOnce](2026-11-19-pc-info-activacion-clickonce.md#tarea-2--activación-completa).

- [x] Organizar la automatización sync con POM y compartir CP-PI-01 entre el paquete publicado y el wheel local. [Comandos de ejecución](../../../demo/README.md#ejecutar-pc-info-automation).
- [x] Verificar el clic de CP-PI-02 en la bandeja de descargas de Edge con el filtro por prefijo. Las comprobaciones por interfaz y navegador se registran en el [plan de activación ClickOnce](2026-11-19-pc-info-activacion-clickonce.md#tarea-1--clic-de-apertura-desde-el-navegador).
- [x] Verificar captura, edición, cierre, retorno a la web y contenido textual del PDF en CP-PI-01 sync completo (Edge + ClickOnce + `SScreen` opcional mediante `window(has_text=...)`).
- [ ] Verificar la API async con los resultados esperados de [CP-PI-01](../../../demo/pc-info-automation/test_plan.md#caso-cp-pi-01--confirmar-datos-y-generar-el-pdf). Las pruebas de la librería ya cubren el clic nativo, la cancelación y la respuesta del bucle.
- [x] Verificar una página creada antes y otra después de importar la extensión. Ambas páginas reales de Playwright expusieron `page.desktop` tras la importación.

## Tarea 7 — compatibilidad, rendimiento y revisión final

**Archivos.** `lib/pyproject.toml`, `lib/README.md`, README principal y documentación técnica.

- [ ] Completar la matriz del wheel con CPython 3.10 x64. Conservar como referencia las verificaciones con Python 3.14 y Playwright 1.65.2 y ajustar los rangos declarados según los resultados.
- [ ] Medir inicialización, búsquedas, lecturas, ediciones y acciones del motor en release, con el formulario Java ya abierto. **Cierre.** Registrar p50/p95, número de muestras, condiciones reproducibles y liberación de sesiones, objetos JAB e hilos tras operaciones repetidas.
- [ ] Revisar el contrato público y la documentación. **Cierre.** Instalación, API sync/async, configuración del runtime, tipado, errores y límites de sesión coinciden con las comprobaciones registradas. Hacer una revisión independiente de ABI, propiedad de recursos, colisiones y efectos tardíos y repetir las comprobaciones afectadas.

## Resultado esperado

Una librería pequeña que añade `page.desktop` al importar y completa el caso Java de la demo utilizando interfaces de accesibilidad reales. El criterio de entrega incluye wheel instalable y recorridos sync y async comprobados; compilar el crate o manipular directamente la API HTTP de la demo no es suficiente.
