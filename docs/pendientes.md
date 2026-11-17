# Estado y pendientes

Corte del 17 de noviembre de 2026. La implementación queda guardada como base inicial; las siguientes tareas deben resolverse por separado para mantener cambios revisables.

## Verificado en este corte

- Wheel Windows x64 construido e instalado en Python global: Python 3.14.4, Playwright 1.63.0, Rust 1.100.0 y Maturin 1.16.0.
- Parche automático `Page.desktop`, fachadas sync/async, errores y validaciones, binding PyO3 y worker Rust con Java Access Bridge.
- 19 pruebas Python y 4 pruebas Rust aprobadas; layout del ABI contrastado con headers reales del JDK.
- Comprobación previa: seis casos Java directos pasaron con OpenJDK 11. Esos casos técnicos se retiraron de la demo al simplificar su alcance a un único escenario funcional POM; este resultado es evidencia histórica, no una suite actual de la demo.
- Page Object web verificado en Edge headless: locators, solicitud, lectura de resultados y descarga/contenido PDF. Se simuló la confirmación HTTP de Java y se evitó la activación nativa; esta comprobación es parcial.
- Comprobación previa del controlador oficial ClickOnce: descargó la publicación e inició el lanzador y Java. La captura posterior de la ventana agotó el plazo. El diagnóstico auxiliar se retiró de la demo simplificada.
- Comando simplificado verificado: `py demo/pc-info-automation/run.py`. Detectó el runtime publicado, arrancó web/Edge y creó la operación. El único escenario terminó por timeout esperando Java, con código 1, evidencia guardada y cierre del navegador y servidor. No se añadieron tests del runner ni de los Page Objects.
- Uso explícito de la API verificado con Java real: configuración y captura desde `page.desktop`, Page Object construido con `page.desktop.window()`, edición, lectura, confirmación, cierre y PDF correctos. Fue una comprobación puntual con arranque Java directo; no se añadió otro escenario a la demo ni se acreditó la activación desde Edge.
- Demo y automatización renombradas a `pc-info` y `pc-info-automation`. Eliminados 1.480 archivos generados (381,6 MB). El comando con la ruta nueva se comprobó hasta el timeout de Java: cerró Edge, eliminó su perfil temporal y no creó `output/` ni cachés Python. La ejecución ya no guarda evidencias automáticamente.

El flujo normal Edge → ClickOnce → Java → web → PDF todavía no está aprobado. El inicio por el controlador del sistema es una comprobación parcial y no acredita la activación desde el navegador. Tampoco se ha completado la revisión visual del PDF.

## Próximas tareas pequeñas

| ID | Tarea | Criterio de cierre |
| --- | --- | --- |
| P01 | Añadir regresiones del ciclo de vida del motor. | Una petición corta en cola no invalida otra activa con plazo vigente; inicio async lento no bloquea el bucle; cancelar la preparación libera la reserva; una raíz Java obsoleta libera su ventana. |
| P02 | Cerrada: comprobación Java directa previa. | Evidencia histórica conservada; casos técnicos retirados de la automatización de ejemplo. |
| P03 | Diagnosticar la captura de Java tras ClickOnce. | Identificar título, tiempos y errores de Java; capturar su ventana y limpiar solo los procesos creados por el flujo. |
| P04 | Resolver la activación normal desde Edge. | El botón web inicia ClickOnce en el único escenario funcional; cualquier intervención necesaria queda explícita. |
| P05 | Completar el recorrido web síncrono. | Editar Java, confirmar, verificar los datos y revisar el PDF mostrado en la web. |
| P06 | Completar el recorrido web asíncrono. | Mismo resultado, con comprobaciones de respuesta del bucle y cancelación. |
| P07 | Verificar versiones declaradas. | Construcción/importación en Python 3.10 y comprobación de Playwright 1.65.2; ajustar rangos a la evidencia. |
| P08 | Medir rendimiento y recursos. | Registrar latencias p50/p95 y liberación de recursos con un escenario reproducible. |
| P09 | Revisar el contrato y preparar la entrega. | Documentación, errores y límites coinciden con las verificaciones realizadas. |

Empezar por P01, con un commit acotado como `test: cubrir plazos y ciclo de vida del motor`. Cada tarea debe cerrar con sus comprobaciones, estado actualizado y un commit breve en español, siguiendo Conventional Commits y sin cuerpo. No agrupar de nuevo todos los hitos.

## Referencias

- [Plan detallado de referencia](superpowers/plans/2026-11-17-libreria-python-rust.md).
- [Arquitectura](superpowers/specs/2026-11-17-libreria-python-rust-design.md).
- [Preparación y pruebas de la demo](../demo/README.md).
