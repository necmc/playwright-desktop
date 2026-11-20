# Plan de pruebas de pc-info

## Objetivo y alcance

Validar el recorrido de extremo a extremo desde el navegador hasta el formulario Java y de vuelta a la web. El resultado esperado es que la fecha, el nombre del equipo y el alias confirmados en el formulario coincidan con los que muestra la web y contiene el PDF.

La versión publicada ejecuta el recorrido completo con datos válidos. El wheel local ejecuta el caso del aviso nativo del navegador.

## Criterios de entrada y ejecución

Usa una sesión interactiva de Windows y un navegador compatible con Playwright. Deja `pc-info` levantada en `http://127.0.0.1:18765` según las [instrucciones de arranque](../README.md#levantar-pc-info). CP-PI-01 requiere que el navegador pueda activar ClickOnce.

Usa la versión publicada para CP-PI-01 y el wheel local para CP-PI-02. [Comandos de ejecución](../README.md#ejecutar-pc-info-automation)

Si la web no está disponible, el runner muestra «El sistema no responde.» y termina antes de abrir el navegador. Registra el caso como **bloqueado por el entorno**.

## Caso CP-PI-01 · Confirmar datos y generar el PDF

Datos de prueba: fecha `2026-11-19`, equipo `EQUIPO-CLICKONCE` y alias `Integración Ñ`. El alias permite detectar pérdida de caracteres durante el recorrido.

| Paso | Resultado esperado |
| --- | --- |
| Pulsa **Obtener información**. Si el navegador solicita abrir el archivo, confirma la apertura. | Se abre una nueva ventana **Información del equipo**. |
| Edita los tres campos y pulsa **Confirmar**. | La ventana Java se cierra. |
| Revisa el resultado en la web. | La página muestra los tres valores ingresados, el mensaje de confirmación y el visor PDF. |
| Lee el PDF generado. | Su texto contiene los tres valores ingresados. |

**Criterio de salida.** CP-PI-01 se aprueba cuando se cumplen todos los resultados esperados. En la revisión de la demo, abre también la vista previa y comprueba que los tres valores sean legibles.

**Evidencia.** Registra el resultado, el navegador y la versión y origen de la librería (PyPI o local). Si falla, anota el paso, el resultado esperado y el observado, y la salida de consola.

## Caso CP-PI-02 · Abrir ClickOnce desde el navegador

Pulsa **Obtener información** y usa `page.desktop` para accionar la apertura en la interfaz del navegador seleccionado. El caso termina al accionar ese control. Registra el navegador y la interfaz utilizada.

El corte verificado cubre el aviso reciente de Edge y el historial de descargas de Chrome.

## Estructura POM

| Ruta | Responsabilidad |
| --- | --- |
| `run.py` | Verifica la disponibilidad de la demo y ejecuta el caso de la versión seleccionada. |
| `tests/test_flow.py` | Define el recorrido y sus resultados esperados. Captura la ventana con `page.desktop.expect_window()` y la entrega al Page Object con `opened.value`. |
| `tests/test_clickonce.py` | Prueba el clic de apertura en el navegador con el wheel local. |
| `pages/` | Contiene los objetos de la página web, del formulario Java y del aviso del navegador. |
| `models/` | Contiene `EquipmentInformation`, el DTO con los tres datos que se comparan a lo largo del flujo. |
| `fixtures/` | Selecciona el navegador y libera los recursos de cada prueba. |
| `utils/` | Extrae el texto del PDF para comprobar su contenido. |
| `config.py` | Define la URL y el plazo de espera, y lee el runtime de la demo. |

Los objetos encapsulan las interacciones con cada pantalla y esperan condiciones observables, como la visibilidad del PDF o el cierre del formulario. El caso expresa qué valores y mensajes deben verse. El navegador usa un perfil aislado, temporal salvo que se prepare uno para extensiones de ClickOnce. El servidor de la demo sigue activo al terminar la prueba.
