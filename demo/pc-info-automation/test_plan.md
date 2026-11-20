# Plan de pruebas de pc-info

## Objetivo y alcance

Validar el recorrido de extremo a extremo desde el navegador hasta el formulario Java y de vuelta a la web. El resultado esperado es que la fecha, el nombre del equipo y el alias confirmados en el formulario coincidan con los que muestra la web y contiene el PDF.

El recorrido CP-PI-01 es el mismo con el paquete de PyPI y con el wheel local. [Comandos de ejecución](../README.md#ejecutar-pc-info-automation).

## Criterios de entrada y ejecución

Usa una sesión interactiva de Windows con Microsoft Edge y ClickOnce habilitado. Deja `pc-info` en ejecución y define `JAVA_HOME` con el JDK de la demo. [Instrucciones de arranque](../README.md#levantar-pc-info).

Registra el caso como **bloqueado por el entorno** si la web no responde, Edge no está instalado o un aviso de seguridad impide abrir Java.

## Caso CP-PI-01 · Confirmar datos y generar el PDF

Datos de prueba: fecha `2026-11-19`, equipo `EQUIPO-CLICKONCE` y alias `Integración Ñ`. El alias permite detectar pérdida de caracteres durante el recorrido.

| Paso | Resultado esperado |
| --- | --- |
| La prueba pulsa **Obtener información**. Si Edge solicita abrir el archivo, pulsa **Abrir** y revisa los avisos de ClickOnce. | Se abre una nueva ventana **Información del equipo**. |
| Edita los tres campos y pulsa **Confirmar**. | La ventana Java se cierra. |
| Revisa el resultado en la web. | La página muestra los tres valores ingresados, el mensaje de confirmación y el visor PDF. |
| Lee el PDF generado. | Su texto contiene los tres valores ingresados. |

**Criterio de salida.** CP-PI-01 se aprueba cuando se cumplen todos los resultados esperados. En la revisión de la demo, abre también la vista previa y comprueba que los tres valores sean legibles.

**Evidencia.** Registra el resultado, el navegador y la versión y origen de la librería (PyPI o local). Si falla, anota el paso, el resultado esperado y el observado, y la salida de consola.

## Caso CP-PI-02 · Abrir ClickOnce desde el navegador

Verificación independiente que requiere una versión con soporte nativo de Windows. La prueba pulsa **Obtener información** y usa `page.desktop` para accionar **Abrir** en el aviso de Edge o **Abrir archivo** en Descargas. El caso termina al accionar ese control. Registra cuál apareció.

## Estructura POM

| Ruta | Responsabilidad |
| --- | --- |
| `run.py` | Verifica la disponibilidad de la demo y ejecuta CP-PI-01. |
| `tests/test_flow.py` | Define el recorrido y sus resultados esperados. Captura la ventana con `page.desktop.expect_window()` y la entrega al Page Object con `opened.value`. |
| `tests/test_clickonce.py` | Contiene la verificación independiente del clic de apertura en Edge. |
| `pages/` | Contiene los objetos de la página web, del formulario Java y del aviso del navegador. |
| `models/` | Contiene `EquipmentInformation`, el DTO con los tres datos que se comparan a lo largo del flujo. |
| `fixtures/` | Abre Edge con un perfil temporal y libera los recursos de cada prueba. |
| `utils/` | Extrae el texto del PDF para comprobar su contenido. |
| `config.py` | Define la URL de la demo y el plazo de activación. |

Los objetos encapsulan las interacciones con cada pantalla y esperan condiciones observables, como la visibilidad del PDF o el cierre del formulario. El caso expresa qué valores y mensajes deben verse. El servidor de la demo sigue activo al terminar la prueba.
