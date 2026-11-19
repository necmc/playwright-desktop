# Plan de pruebas de pc-info

## Objetivo y alcance

Validar el recorrido de extremo a extremo desde el navegador hasta el formulario Java y de vuelta a la web. El resultado esperado es que la fecha, el nombre del equipo y el alias confirmados en el formulario coincidan con los que muestra la web y contiene el PDF.

Se ejecuta un caso funcional de caja negra con datos válidos en un navegador por ejecución. Quedan fuera las entradas inválidas, la cancelación, una matriz de navegadores y la comparación visual del PDF. El caso se basa en el [recorrido de la demo](../README.md#levantar-pc-info).

## Criterios de entrada y ejecución

Usa una sesión interactiva de Windows, un navegador compatible con Playwright y las dependencias del [README de la demo](../README.md). Deja `pc-info` levantada con Java Access Bridge habilitado en `http://127.0.0.1:18765`. Acepta la apertura de ClickOnce si el navegador la solicita.

Desde la raíz del repositorio, ejecuta:

```powershell
py demo/pc-info-automation/run.py
```

Si la web no está disponible, el runner muestra «El sistema no responde.» y termina antes de abrir el navegador. Registra el caso como **bloqueado por el entorno**.

## Caso PC-01 · Confirmar datos y generar el PDF

Datos de prueba: fecha `2026-11-19`, equipo `EQUIPO-CLICKONCE` y alias `Integración Ñ`. El alias permite detectar pérdida de caracteres durante el recorrido.

| Paso | Resultado esperado |
| --- | --- |
| Abre la demo y solicita la información desde el navegador. | Se abre una nueva ventana **Información del equipo**. |
| Edita los tres campos y pulsa **Confirmar**. | La ventana Java se cierra. |
| Revisa el resultado en la web. | La página muestra los tres valores ingresados, el mensaje de confirmación y el visor PDF. |
| Lee el PDF generado. | Su texto contiene los tres valores ingresados. |

**Criterio de salida.** PC-01 se aprueba cuando se cumplen todos los resultados esperados. Si el escenario inicia y falla, registra el paso, los datos, el resultado esperado y el observado, la salida de consola y la versión de la demo y del navegador. Distingue, por ejemplo, un fallo de apertura de ClickOnce de una diferencia en el contenido del PDF.

**Última verificación local (19 de noviembre de 2026).** PC-01 agotó la espera de la ventana Java tras solicitar la apertura desde Edge. Al abrir el formulario Java directamente en una comprobación aparte, pasaron la captura, la edición, el cierre y las comprobaciones de la web y del PDF.

## Estructura POM

| Ruta | Responsabilidad |
| --- | --- |
| `run.py` | Verifica la disponibilidad de la demo y ejecuta el caso sin parámetros. |
| `tests/test_flow.py` | Define los datos, el recorrido y los resultados esperados. Captura la ventana con `page.desktop.expect_window()`. |
| `pages/` | `InformationPage` concentra locators y acciones de la web. `InformationForm` maneja los controles de la ventana Java capturada. |
| `models/` | Contiene `EquipmentInformation`, el DTO con los tres datos que se comparan a lo largo del flujo. |
| `fixtures/` | Selecciona y prepara el navegador, y libera los recursos de cada prueba. |
| `utils/` | Extrae el texto del PDF para comprobar su contenido. |
| `config.py` | Define la URL y el plazo de espera, y lee el runtime de la demo. |

Los objetos encapsulan las interacciones con cada pantalla y esperan condiciones observables, como la visibilidad del PDF o el cierre del formulario. El caso expresa qué valores y mensajes deben verse. El navegador usa un perfil aislado, temporal salvo que se prepare uno para extensiones de ClickOnce. El servidor de la demo sigue activo al terminar la prueba.
