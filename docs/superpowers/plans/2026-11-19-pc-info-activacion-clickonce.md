# pc-info — activación ClickOnce desde la automatización

**Objetivo.** Verificar el clic de apertura con el navegador seleccionado y registrar por separado la activación completa de ClickOnce.

**Arquitectura.** Playwright pulsa **Obtener información** y el JavaScript de la demo solicita la activación. El Page Object localiza la ventana del navegador con `page.desktop` y acciona el control de apertura.

**Dependencia.** CP-PI-02 requiere el [soporte nativo de Windows](2026-11-19-windows-nativo-uia.md) y se ejecuta con un perfil de pruebas temporal. [Instrucciones de ejecución](../../../demo/README.md#ejecutar-pc-info-automation).

**Tecnologías.** Windows x64, navegador compatible con ClickOnce, .NET Framework 4.8, Mage, Playwright Python y Java Swing.

**Referencias.** [Diseño de pc-info](../specs/2026-11-17-demo-clickonce-java-design.md) y [caso CP-PI-01](../../../demo/pc-info-automation/test_plan.md#caso-cp-pi-01--confirmar-datos-y-generar-el-pdf).

**Estado.** En validación. CP-PI-02 pasó tres veces con el aviso reciente de Edge y tres veces con el historial de descargas de Chrome. CP-PI-01 sigue pendiente.

## Tarea 1 — clic de apertura desde el navegador

**Archivos.** `demo/pc-info-automation/pages/browser_clickonce.py`, `fixtures/` y `tests/test_clickonce.py`.

- [x] Pulsar **Obtener información** una sola vez y resolver la ventana mediante `page.desktop.window(pid=pid, title_prefix=...)`. El prefijo usa el título de la pestaña y el separador que precede al nombre del navegador o perfil.
- [x] Accionar **Abrir archivo** en el aviso reciente de Edge y el enlace normal del manifiesto en el historial de Chrome.
- [ ] Validar el aviso de apertura directa de Edge.
- [ ] Reabrir el panel **Descargas** de Edge cuando el aviso reciente ya no está visible e identificar el control de apertura del manifiesto sin ambigüedad.
- [ ] Verificar CP-PI-02 en Brave y en otros navegadores compatibles. Registrar si cada navegador presenta el manifiesto y qué control permite abrirlo.

## Tarea 2 — activación completa

**Archivos.** `demo/pc-info-automation/pages/information_page.py`, `demo/pc-info-automation/tests/test_flow.py` y `demo/pc-info-automation/test_plan.md`.

- [ ] Confirmar que al abrir el manifiesto desde el navegador, el lanzador recibe el parámetro `operation` mediante `ActivationUri` y se captura la ventana Java correspondiente con `page.desktop.expect_window()`. Si Java no abre, registrar hasta qué paso avanzó la activación. [Parámetros HTTP en ClickOnce](https://learn.microsoft.com/en-us/visualstudio/deployment/how-to-retrieve-query-string-information-in-an-online-clickonce-application?view=vs-2022).
- [ ] Completar CP-PI-01 y comprobar que los tres valores editados en Java coincidan con la web y sean legibles en el visor PDF. [Plan de pruebas](../../../demo/pc-info-automation/test_plan.md#caso-cp-pi-01--confirmar-datos-y-generar-el-pdf).

## Tarea 3 — aviso de SmartScreen

**Archivo.** `demo/pc-info-automation/test_plan.md`.

- [ ] Si SmartScreen interrumpe la activación, registrar CP-PI-01 como bloqueado e identificar el ejecutable mostrado. [Reputación de SmartScreen](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation).

## Criterio de cierre

Registrar el resultado por caso, navegador e interfaz de apertura. CP-PI-01 requiere evidencia de Java y del PDF. CP-PI-02 termina al accionar el control de apertura del navegador.
