# Plan de pruebas de pc-info

## Objetivo y alcance

Validar el recorrido de extremo a extremo desde el navegador hasta el formulario Java y de vuelta a la web. El resultado esperado es que la fecha, el nombre del equipo y el alias confirmados en el formulario coincidan con los que muestra la web y contiene el PDF.

Se ejecuta un caso funcional de caja negra con datos válidos en un navegador por ejecución. Quedan fuera las entradas inválidas, la cancelación, una matriz de navegadores y la comparación del diseño del PDF.

## Criterios de entrada y ejecución

Usa una sesión interactiva de Windows y un navegador compatible con Playwright y ClickOnce. Deja `pc-info` levantada en `http://127.0.0.1:18765` según las [instrucciones de arranque](../README.md#levantar-pc-info). Acepta la apertura de ClickOnce si el navegador la solicita.

Ejecuta el mismo caso con la versión publicada o con el wheel local. Usa el wheel local para validar cambios de la librería. [Comandos de ejecución](../README.md#ejecutar-pc-info-automation)

Si la web no está disponible, el runner muestra «El sistema no responde.» y termina antes de abrir el navegador. Registra el caso como **bloqueado por el entorno**.

## Caso PC-01 · Confirmar datos y generar el PDF

Datos de prueba: fecha `2026-11-19`, equipo `EQUIPO-CLICKONCE` y alias `Integración Ñ`. El alias permite detectar pérdida de caracteres durante el recorrido.

| Paso | Resultado esperado |
| --- | --- |
| Abre la demo y solicita la información desde el navegador. | Se abre una nueva ventana **Información del equipo**. |
| Edita los tres campos y pulsa **Confirmar**. | La ventana Java se cierra. |
| Revisa el resultado en la web. | La página muestra los tres valores ingresados, el mensaje de confirmación y el visor PDF. |
| Lee el PDF generado. | Su texto contiene los tres valores ingresados. |

**Criterio de salida.** PC-01 se aprueba cuando se cumplen todos los resultados esperados. En la revisión de la demo, abre también la vista previa y comprueba que los tres valores sean legibles.

**Evidencia.** Registra el resultado, el navegador y la versión y origen de la librería (PyPI o local). Si falla, anota el paso, el resultado esperado y el observado, y la salida de consola.

## Estructura POM

| Ruta | Responsabilidad |
| --- | --- |
| `run.py` | Verifica la disponibilidad de la demo y ejecuta el caso sin parámetros. |
| `tests/test_flow.py` | Define el recorrido y sus resultados esperados. Captura la ventana con `page.desktop.expect_window()` y la entrega al Page Object con `opened.value`. |
| `pages/` | `InformationPage` concentra locators y acciones de la web. `InformationForm` maneja los controles de la ventana Java capturada. |
| `models/` | Contiene `EquipmentInformation`, el DTO con los tres datos que se comparan a lo largo del flujo. |
| `fixtures/` | Selecciona el navegador, configura el runtime y libera los recursos de cada prueba. |
| `utils/` | Extrae el texto del PDF para comprobar su contenido. |
| `config.py` | Define la URL y el plazo de espera, y lee el runtime de la demo. |

Los objetos encapsulan las interacciones con cada pantalla y esperan condiciones observables, como la visibilidad del PDF o el cierre del formulario. El caso expresa qué valores y mensajes deben verse. El navegador usa un perfil aislado, temporal salvo que se prepare uno para extensiones de ClickOnce. El servidor de la demo sigue activo al terminar la prueba.
