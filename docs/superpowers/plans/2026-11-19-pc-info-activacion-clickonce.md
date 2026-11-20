# pc-info — activación ClickOnce desde la automatización

**Objetivo.** Validar la apertura y el recorrido completo ClickOnce en Microsoft Edge. La compatibilidad con otros navegadores (Chrome, Brave, etc.) se abordará a futuro en planes independientes.

**Arquitectura.** Playwright pulsa **Obtener información** y el JavaScript de la demo solicita la activación. CP-PI-01 y CP-PI-02 comparten el Page Object que pulsa **Abrir** en Edge. CP-PI-01 contempla la confirmación opcional del instalador antes de esperar Java.

**Dependencia.** La automatización usa Microsoft Edge con un perfil temporal y el [soporte nativo de Windows](2026-11-19-windows-nativo-uia.md). [Instrucciones de ejecución](../../../demo/README.md#ejecutar-pc-info-automation).

**Tecnologías.** Windows x64, navegador compatible con ClickOnce, .NET Framework 4.8, Mage, Playwright Python y Java Swing.

**Referencias.** [Diseño de pc-info](../specs/2026-11-17-demo-clickonce-java-design.md) y [caso CP-PI-01](../../../demo/pc-info-automation/test_plan.md#caso-cp-pi-01--confirmar-datos-y-generar-el-pdf).

**Estado.** Cerrado (alcance Edge). Las tres tareas están completadas con el wheel de desarrollo (`CP-PI-01` completo con y sin `SScreen`, con ClickOnce confirmando u omitiendo el aviso, y `CP-PI-02` en Edge). Otros navegadores quedan para planes futuros.

## Tarea 1 — clic de apertura desde el navegador

**Archivos.** `demo/pc-info-automation/pages/browser_clickonce.py`, `fixtures/browser.py` y `tests/test_clickonce.py`.

Con una versión de la librería que incluya soporte nativo, ejecuta CP-PI-02 desde `demo/pc-info-automation/` con `py -m unittest tests.test_clickonce`.

- [x] Pulsar **Obtener información** una sola vez y resolver la ventana mediante `page.desktop.window(pid=pid, title_prefix=...)`. El prefijo usa el título de la pestaña y el separador que precede al nombre del navegador o perfil.
- [x] Abrir Edge con un perfil temporal y conectar Playwright mediante `connect_over_cdp(no_defaults=True)`. Se comprobó que `DOWNLOAD_BEHAVIOR = None` muestra el aviso nativo y que el valor `"default"` descarga el manifiesto.
- [x] Incorporar los argumentos para evitar el relanzamiento por compatibilidad y la sincronización del perfil. Verificar la conexión con el wheel local y comprobar que se conserva el aviso nativo.
- [x] Accionar **Abrir archivo** en la bandeja de descargas de Edge.
- [x] Admitir `pane` como contenedor de búsqueda de UI Automation. El Page Object acciona **Abrir** en el aviso nativo y el wheel de desarrollo abre el aviso de ClickOnce.

*(Alcance futuro — otros planes: validar la apertura y el recorrido por separado en Chrome —donde ya se verificó el clic en el historial de descargas—, Brave y otros navegadores compatibles, indicando si requieren extensión. Reabrir el panel Descargas de Edge cuando el aviso reciente desaparece queda descartado en el flujo estándar con `DOWNLOAD_BEHAVIOR = None`.)*

## Tarea 2 — activación completa

**Archivos.** `demo/pc-info-automation/pages/information_page.py`, `demo/pc-info-automation/tests/test_flow.py` y `demo/pc-info-automation/test_plan.md`.

Los dos comandos de la demo ejecutan CP-PI-01 mediante `run.py`. La validación en curso usa el wheel de desarrollo.

- [x] Usar en CP-PI-01 el mismo Page Object que acciona **Abrir** en CP-PI-02.
- [x] Confirmar que el lanzador recibe `operation` mediante `ActivationUri` y se captura la ventana Java con `page.desktop.expect_window()`. [Parámetros HTTP en ClickOnce](https://learn.microsoft.com/en-us/visualstudio/deployment/how-to-retrieve-query-string-information-in-an-online-clickonce-application?view=vs-2022).
- [x] Completar CP-PI-01 y comprobar que los tres valores editados en Java coincidan con la web y sean legibles en el visor PDF. [Plan de pruebas](../../../demo/pc-info-automation/test_plan.md#caso-cp-pi-01--confirmar-datos-y-generar-el-pdf).

## Tarea 3 — advertencias de seguridad

**Archivos.** Page Objects en `demo/pc-info-automation/pages/` (`clickonce_installer.py`, `sscreen_warning.py`) y `test_plan.md`.

- [x] Registrar que CP-PI-01 llegó a **Ejecución de la aplicación - Advertencia de seguridad** de ClickOnce después del aviso de Edge.
- [x] Identificar el aviso nuevo por su título y verificar que muestra **Información del equipo** y el origen de la demo. El wheel de desarrollo acciona **Ejecutar** cuando ambos coinciden.
- [x] Verificar el recorrido cuando ClickOnce omite la confirmación (`confirm_if_present()` alterna con la detección directa del formulario Java).
- [x] Gestionar `SScreen` cuando aparece mediante `page.desktop.window(has_text=...)` (`SScreenWarning`), saliendo de inmediato cuando Java abre directamente.

## Criterio de cierre

Registrar el resultado por caso, navegador e interfaz de apertura. CP-PI-01 requiere evidencia de Java y del PDF. CP-PI-02 termina al accionar el control de apertura del navegador.
