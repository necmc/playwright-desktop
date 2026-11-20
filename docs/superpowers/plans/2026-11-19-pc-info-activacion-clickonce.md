# pc-info — activación ClickOnce desde la automatización

**Objetivo.** Validar primero la apertura ClickOnce en Edge y comprobar por separado los demás navegadores compatibles.

**Arquitectura.** Playwright pulsa **Obtener información** y el JavaScript de la demo solicita la activación. CP-PI-01 y CP-PI-02 comparten el Page Object que pulsa **Abrir** en Edge. CP-PI-01 contempla la confirmación opcional del instalador antes de esperar Java.

**Dependencia.** La automatización usa Microsoft Edge con un perfil temporal y el [soporte nativo de Windows](2026-11-19-windows-nativo-uia.md). [Instrucciones de ejecución](../../../demo/README.md#ejecutar-pc-info-automation).

**Tecnologías.** Windows x64, navegador compatible con ClickOnce, .NET Framework 4.8, Mage, Playwright Python y Java Swing.

**Referencias.** [Diseño de pc-info](../specs/2026-11-17-demo-clickonce-java-design.md) y [caso CP-PI-01](../../../demo/pc-info-automation/test_plan.md#caso-cp-pi-01--confirmar-datos-y-generar-el-pdf).

**Estado.** En validación. Con el wheel de desarrollo, CP-PI-01 pulsa **Abrir** en Edge y **Ejecutar** en el aviso de ClickOnce de la demo. La prueba continúa esperando la ventana Java, que aún no aparece dentro del plazo.

## Tarea 1 — clic de apertura desde el navegador

**Archivos.** `demo/pc-info-automation/pages/browser_clickonce.py`, `fixtures/browser.py` y `tests/test_clickonce.py`.

Con una versión de la librería que incluya soporte nativo, ejecuta CP-PI-02 desde `demo/pc-info-automation/` con `py -m unittest tests.test_clickonce`.

- [x] Pulsar **Obtener información** una sola vez y resolver la ventana mediante `page.desktop.window(pid=pid, title_prefix=...)`. El prefijo usa el título de la pestaña y el separador que precede al nombre del navegador o perfil.
- [x] Abrir Edge con un perfil temporal y conectar Playwright mediante `connect_over_cdp(no_defaults=True)`. Se comprobó que `DOWNLOAD_BEHAVIOR = None` muestra el aviso nativo y que el valor `"default"` descarga el manifiesto.
- [x] Incorporar los argumentos para evitar el relanzamiento por compatibilidad y la sincronización del perfil. Verificar la conexión con el wheel local y comprobar que se conserva el aviso nativo.
- [x] Accionar **Abrir archivo** en la bandeja de descargas de Edge.
- [x] Admitir `pane` como contenedor de búsqueda de UI Automation. El Page Object acciona **Abrir** en el aviso nativo y el wheel de desarrollo abre el aviso de ClickOnce.
- [ ] Reabrir el panel **Descargas** de Edge cuando el aviso reciente ya no está visible e identificar el control de apertura del manifiesto sin ambigüedad.
- [ ] Validar la apertura y el recorrido completo por separado en Chrome, Brave y otros navegadores compatibles. Registrar cómo abre el manifiesto cada navegador y si requiere una extensión para activar ClickOnce. En Chrome ya se verificó el clic en el historial de descargas.

## Tarea 2 — activación completa

**Archivos.** `demo/pc-info-automation/pages/information_page.py`, `demo/pc-info-automation/tests/test_flow.py` y `demo/pc-info-automation/test_plan.md`.

Los dos comandos de la demo ejecutan CP-PI-01 mediante `run.py`. La validación en curso usa el wheel de desarrollo.

- [x] Usar en CP-PI-01 el mismo Page Object que acciona **Abrir** en CP-PI-02.
- [ ] Confirmar que el lanzador recibe `operation` mediante `ActivationUri` y se captura la ventana Java con `page.desktop.expect_window()`. [Parámetros HTTP en ClickOnce](https://learn.microsoft.com/en-us/visualstudio/deployment/how-to-retrieve-query-string-information-in-an-online-clickonce-application?view=vs-2022).
- [ ] Completar CP-PI-01 y comprobar que los tres valores editados en Java coincidan con la web y sean legibles en el visor PDF. [Plan de pruebas](../../../demo/pc-info-automation/test_plan.md#caso-cp-pi-01--confirmar-datos-y-generar-el-pdf).

## Tarea 3 — advertencias de seguridad

**Archivos.** Page Object del instalador en `demo/pc-info-automation/pages/` y `test_plan.md`.

- [x] Registrar que CP-PI-01 llegó a **Ejecución de la aplicación - Advertencia de seguridad** de ClickOnce después del aviso de Edge.
- [x] Identificar el aviso nuevo por su título y verificar que muestra **Información del equipo** y el origen de la demo. El wheel de desarrollo acciona **Ejecutar** cuando ambos coinciden.
- [ ] Verificar el recorrido cuando ClickOnce omite la confirmación.
- [ ] Registrar un bloqueo separado si aparece Microsoft Defender SmartScreen. [Reputación de SmartScreen](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

## Criterio de cierre

Registrar el resultado por caso, navegador e interfaz de apertura. CP-PI-01 requiere evidencia de Java y del PDF. CP-PI-02 termina al accionar el control de apertura del navegador.
