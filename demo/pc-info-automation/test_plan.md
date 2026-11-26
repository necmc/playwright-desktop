# Plan de pruebas de pc-info

## Objetivo y alcance

Validar el recorrido de extremo a extremo desde el navegador hasta el formulario Java, la revisión de datos en la web y la vista del PDF. El resultado esperado es que la vista PDF permanezca oculta al inicio y al regresar de Java, que el formulario web muestre la fecha, el nombre del equipo y el alias confirmados con el botón **Continuar**, y que al pulsarlo se pase a la vista del PDF cuyo texto contiene los tres valores.

El recorrido CP-PI-01 es el mismo con el paquete de PyPI y con el wheel local. [Comandos de ejecución](../README.md#ejecutar-pc-info-automation).

## Criterios de entrada y ejecución

Usa una sesión interactiva de Windows con Microsoft Edge y ClickOnce habilitado. Deja `pc-info` en ejecución y define `JAVA_HOME` con el JDK de la demo. [Instrucciones de arranque](../README.md#levantar-pc-info).

Registra el caso como **bloqueado por el entorno** si la web no responde, Edge no está instalado o un aviso de seguridad impide abrir Java.

Si aparece SScreen, revisa el proceso de automatización.

## Caso CP-PI-01 · Confirmar datos y generar el PDF

Datos de prueba: fecha `2026-11-19`, equipo `EQUIPO-CLICKONCE` y alias `Integración Ñ`. El alias permite detectar pérdida de caracteres durante el recorrido.

| Paso | Resultado esperado |
| --- | --- |
| La prueba pulsa **Obtener información** y **Abrir** en Edge. | ClickOnce inicia la activación de la aplicación. |
| Si ClickOnce muestra la confirmación, la prueba valida el nombre de la demo y su origen y pulsa **Ejecutar**. | Se abre una nueva ventana **Información del equipo**. |
| Edita los tres campos y pulsa **Confirmar**. | La ventana Java se cierra. |
| Revisa el resultado en la web y pulsa **Continuar**. | Antes de pulsar **Continuar**, la página muestra los tres valores ingresados, el mensaje de confirmación y el botón **Continuar** sin mostrar el visor PDF. Al pulsar **Continuar**, pasa a la vista dedicada del visor PDF. |
| Lee el PDF generado. | Su texto contiene los tres valores ingresados. |

**Criterio de salida.** CP-PI-01 se aprueba cuando se cumplen todos los resultados esperados. En la revisión manual de la demo, verifica también que al pulsar **Continuar** el PDF sea legible en el visor y que **Volver** regrese al formulario limpio.

**Evidencia.** Registra el resultado, el navegador y la versión y origen de la librería (PyPI o local). Si falla, anota el paso, el resultado esperado y el observado, y la salida de consola.

## Caso CP-PI-02 · Abrir ClickOnce desde el navegador

Verificación independiente que requiere una versión con soporte nativo de Windows. La prueba pulsa **Obtener información** dentro de `page.expect_app(...)`, usa el `DesktopContext` (`dcontext.app(...)`) para accionar **Abrir** en el aviso de Edge o **Abrir archivo** en Descargas y cierra el aviso capturado con `close()`. Registra cuál apareció.

## Estructura POM

| Ruta | Responsabilidad |
| --- | --- |
| `run.py` | Verifica la disponibilidad de la demo y ejecuta CP-PI-01. |
| `tests/test_flow.py` | Define el recorrido CP-PI-01 leyendo Page Objects en secuencia de negocio, manteniendo `page.expect_app()` visible para capturar la ventana Java. |
| `tests/test_clickonce.py` | Contiene la verificación independiente CP-PI-02 del clic de apertura en Edge usando `InformationPage` y `BrowserOpenDialog`. |
| `pages/web/information_page.py` | `InformationPage`: formulario web inicial (**Obtener información**, espera y lectura de datos recibidos, transición a `PdfPreviewPage` al pulsar **Continuar**). |
| `pages/web/pdf_preview_page.py` | `PdfPreviewPage`: vista dedicada del visor PDF (visibilidad, descarga y lectura de texto, regreso con **Volver**). |
| `pages/desktop/desktop_form_window.py` | `DesktopFormWindow`: ventana Java de escritorio (detección de disponibilidad, llenado de campos y confirmación). |
| `pages/dialogs/browser_open_dialog.py` | `BrowserOpenDialog`: aviso nativo de apertura o enlace de descarga en Edge. |
| `pages/dialogs/clickonce_dialog.py` | `ClickOnceDialog`: advertencia de seguridad de ClickOnce (validación de origen/nombre y botón **Ejecutar**). |
| `pages/dialogs/sscreen_dialog.py` | `SScreenDialog`: advertencia de Microsoft Defender SmartScreen (**Más información** y **Ejecutar de todas formas**). |
| `models/equipment_information.py` | `EquipmentInformation`: DTO con fecha, equipo y alias comparados entre desktop, web y PDF. |
| `fixtures/` | Abre Edge con un perfil temporal, inicializa `info_page` y `pdf_page` y libera los recursos de cada prueba. |
| `utils/pdf.py` | Extrae texto del PDF usado internamente por `PdfPreviewPage.read_text()`. |
| `config.py` | Define la URL de la demo, los plazos de activación y las pausas configurables. |

Los objetos encapsulan los locators y las esperas observables de cada pantalla o diálogo; las pruebas expresan únicamente los pasos funcionales y aserciones del caso. El servidor de la demo sigue activo al terminar la prueba.
