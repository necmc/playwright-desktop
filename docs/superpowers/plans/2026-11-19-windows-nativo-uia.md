# playwright-desktop — soporte básico de Windows nativo

**Objetivo.** Usar `page.desktop` para localizar una ventana nativa de Windows, accionar sus controles accesibles y comprobar el resultado con una aplicación controlada.

**Arquitectura.** Añadir Windows UI Automation (UIA) al wheel existente. La librería localiza y reserva la ventana antes de resolver en Rust qué proveedor expone sus controles. Java Access Bridge (JAB) y UIA tienen hilos separados. Python comprueba la compatibilidad del entorno al cargar la extensión.

**Tecnologías.** Windows x64, Python 3.10+, PyO3 y `windows` 0.62.2. UIA usa COM de Windows. [Bindings de Windows UI Automation](https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/UI/Accessibility/index.html)

**Referencias.** [Arquitectura de la librería](../specs/2026-11-17-libreria-python-rust-design.md) y [activación ClickOnce en pc-info](2026-11-19-pc-info-activacion-clickonce.md).

**Estado.** Cerrado. Las siete tareas están completadas. `page.desktop` selecciona JAB o UIA por ventana y permite localizar una ventana existente por título exacto, prefijo o PID.

## Alcance y contrato

- Mantener `page.desktop.window()` y `page.desktop.expect_window()` con la sintaxis actual. El motor se selecciona internamente por ventana. Una misma `Page` puede manejar una ventana nativa y otra Swing con el mismo contrato.
- Usar `window()` para una ventana existente y `expect_window()` para una ventana de nivel superior que aparezca durante la acción. La captura identifica la ventana y la primera búsqueda de un control confirma el proveedor. `window(pid=pid)` admite un proceso con una única ventana visible con título. Las capturas conservan el título exacto, el filtro por PID, `.value` y `wait_for_closed()`.
- Buscar los avisos y paneles integrados dentro de su ventana. La búsqueda anidada con `get_by_role()` admite `dialog`, `group`, `pane`, `menu` y `listitem`. `wait_for()` espera que el control sea visible.
- Admitir `button`, `link` y `menuitem` por nombre accesible exacto. `click()` invoca la acción UIA del control si expone `InvokePattern`. El resultado funcional se comprueba esperando una consecuencia observable, como que aparezca o se cierre un aviso.
- Las acciones nativas funcionan sin configurar Java. La configuración existente de `java_home` sigue siendo una pista para localizar el bridge de Swing. `page.desktop.get_by_role()` conserva el comportamiento de usar la última ventana capturada.

```python
window = page.desktop.window(title="Ventana de prueba", pid=pid)
with page.desktop.expect_window(title="Opciones", pid=pid) as opened:
    window.get_by_role("button", name="Mostrar opciones").click()

opened.value.get_by_role("button", name="Aceptar").click()
opened.value.wait_for_closed()
```

Para un aviso incluido en la ventana existente, el localizador se limita al contenedor accesible.

```python
notice = window.get_by_role("group", name="Opciones integradas")
notice.get_by_role("button", name="Aplicar").click()
```

## Tarea 1 — selección automática por ventana

**Archivos.** `lib/python/playwright_desktop/_models.py`, `_platform.py`, `_runtime.py`, `_sync.py`, `_async.py` y `_native.pyi`. En Rust, `lib/rust/src/engine.rs`, `engine/windows_platform.rs` y `backend.rs`.

- [x] Preparar y resolver la captura mediante HWND, PID y título antes de inicializar JAB o UIA. Compartir la identidad y la reserva de la ventana entre los motores para que la misma ventana aparezca una sola vez. Verificar `CP-UN-03`.
- [x] Resolver el proveedor al buscar el primer control. Usar JAB cuando reconoce el HWND y UIA cuando esa comprobación no lo reconoce. El runtime ya cargado, `java_home` o `JAVA_HOME` proporcionan el bridge. Verificar `CP-UN-01`. [Identificación de ventanas mediante Java Access Bridge](https://docs.oracle.com/en/java/javase/21/access/java-access-bridge-api.html)
- [x] Vincular el proveedor a la referencia antes de enviar la primera acción. Compartir el mismo plazo entre las comprobaciones y la acción. Liberar las reservas de la sesión al cerrar `Page`. Cada hilo mantiene su cola, cancelación y protección contra bloqueos. Verificar `CP-UN-02` y `CP-UN-06`.

## Tarea 2 — ventanas y controles mediante UI Automation

**Archivos.** `lib/rust/Cargo.toml`, `lib/rust/src/lib.rs`, `backend.rs`, `windows.rs` y el nuevo `uia.rs`.

- [x] Habilitar los bindings COM y UIA de `windows-rs`. Inicializar COM en MTA dentro del hilo UIA. Confinar cliente y elementos a ese hilo y liberarlos antes de `CoUninitialize`. [Hilos de UI Automation](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-threading)
- [x] Reutilizar la enumeración Win32 de ventanas visibles y obtener el elemento con `ElementFromHandle`. Revalidar HWND, PID e identidad UIA antes de actuar sobre una referencia ya capturada. Si una ventana se cierra o se reemplaza, conservar el error existente. Verificar `CP-UN-03`. [Búsqueda de elementos UIA](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-obtainingelements)
- [x] Buscar los controles desde la raíz de la ventana o desde el contenedor accesible de un popup. Excluir controles fuera de pantalla y esperar a que el control único esté habilitado. Las coincidencias múltiples dentro del mismo alcance producen `AmbiguousMatchError`. Verificar `CP-UN-04`.
- [x] Ejecutar `IUIAutomationInvokePattern::Invoke()` una vez tras comprobar el plazo y la cancelación. Traducir fallos del proveedor o patrón ausente a `DesktopActionError`. Si una llamada ya enviada vence, informar el estado sin repetirla. Verificar `CP-UN-05` y `CP-UN-06`. [Acción Invoke](https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nf-uiautomationclient-iuiautomationinvokepattern-invoke)

## Tarea 3 — pruebas del wheel y aplicación Windows controlada

**Archivos.** `lib/tests/test_windows_uia.py`, `lib/tests/fixtures/` y `lib/README.md`.

- [x] Usar un proceso de prueba Win32 con una ventana principal, un botón que abre un diálogo y un botón del diálogo que registra un resultado y lo cierra. Identificar el proceso por PID y liberar solo el proceso iniciado por la prueba.
- [x] Ejecutar con el wheel una prueba de integración sync y otra async. Comprobar que el primer clic abra el diálogo y el segundo registre el resultado y cierre la ventana. Verificar también un aviso que aparezca dentro de una ventana existente y use un botón con el mismo nombre que otro control fuera del aviso.
- [x] Ejecutar las suites existentes de Python, Rust y Swing. Verificar en una misma página la selección automática para una ventana Win32 y otra Swing. Tras aprobar los casos, actualizar la tabla de `lib/README.md` y agregar un ejemplo breve con la sintaxis común.

## Tarea 4 — separar el coordinador de la plataforma

- [x] Definir el contrato Rust para descubrir e identificar ventanas, preparar capturas, resolver el proveedor y enviar acciones. El coordinador conserva las reservas y los plazos.
- [x] Implementar el contrato con Win32, JAB y UIA. Mantener el contrato Python y las reglas actuales de selección.
- [x] Probar el coordinador con una ventana simulada y ejecutar la regresión de Windows con el wheel.

## Tarea 5 — confirmar el proveedor de cada ventana

- [x] Elegir JAB cuando el bridge reconoce el HWND. Usar el nombre del ejecutable para dar margen al registro de Swing dentro del plazo de detección. Si el bridge no reconoce la ventana, continuar con UIA antes de enviar la acción.
- [x] Verificar el clic en WinForms con un ejecutable llamado `java.exe` y la regresión de captura, edición y clic en Swing.

## Tarea 6 — validación de texto y plazos de escritura

- [x] Mantener en Python la validación de Unicode y trasladar el límite de 1023 unidades UTF-16 a JAB. Conservar `ValueError` para el texto fuera del límite.
- [x] Registrar el inicio de `fill()` y `click()` justo antes de la llamada que modifica el control. Distinguir un timeout previo al inicio de un timeout con resultado incierto.
- [x] Verificar el límite en Swing, la cancelación y el timeout.

## Tarea 7 — filtrar una ventana por prefijo del título

- [x] Incorporar `title_prefix` en `window()` para sync y async y combinarlo con el PID.
- [x] Verificar en Rust que el prefijo conserva la selección única, el filtro por PID y la búsqueda de título exacto.
- [x] Verificar el filtro en la bandeja de descargas de Edge y el historial de Chrome. [Resultados del clic de apertura](2026-11-19-pc-info-activacion-clickonce.md#tarea-1--clic-de-apertura-desde-el-navegador).

## Pruebas unitarias

Usar dobles de proveedores y del worker para verificar el contrato sin navegador, JVM ni ventanas visibles. Extender `lib/tests/test_api.py` y `test_async.py` para la fachada Python, `lib/rust/src/engine/tests.rs` para el worker y el módulo `uia` para los controles nativos.

| Caso | Comprobación | Resultado esperado |
| --- | --- | --- |
| `CP-UN-01` | Detección automática en sync y async. | JAB tiene prioridad cuando reconoce la ventana Swing. Un control nativo usa UIA con `JAVA_HOME` ausente o inválido. Si JAB reconoce una ventana y el contexto accesible sigue inaccesible, se obtiene un diagnóstico de Java. |
| `CP-UN-02` | Una página usa ambas sesiones y luego se cierra. | Cada ventana envía solicitudes solo a su motor. El cierre libera ambas sesiones y sus reservas. |
| `CP-UN-03` | Filtrado por título y PID, ventana previa y HWND reutilizado. | `expect_window()` ignora ventanas previas, deduplica una ventana que ambos motores reconozcan y detecta una referencia obsoleta. |
| `CP-UN-04` | Tipo, nombre, estado y alcance del control en un popup. | Encontrar el botón del popup aunque otro tenga el mismo nombre fuera de él. Un duplicado dentro del popup es ambiguo. Los controles ausentes o deshabilitados respetan el plazo. |
| `CP-UN-05` | Invocación disponible, patrón ausente y error del proveedor. | La acción se envía una sola vez, incluso si el resultado del clic es incierto. Los fallos devuelven `DesktopActionError` con contexto. |
| `CP-UN-06` | Solicitud cancelada, vencida o proveedor bloqueado. | Una solicitud invalidada se descarta antes de invocar. El bucle async sigue respondiendo y un worker UIA bloqueado queda aislado de Java. |

## Resultados de cierre

Pasaron 21 pruebas de Rust y 43 de Python. Las pruebas interactivas comprobaron la apertura y el cierre de un diálogo nativo, el clic dentro de un panel con botones homónimos y el uso de WinForms y Swing desde la misma página.

La verificación del clic en el navegador se registra en el [plan de ClickOnce](2026-11-19-pc-info-activacion-clickonce.md).
