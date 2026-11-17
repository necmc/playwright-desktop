# Demo ClickOnce y Java — plan de implementación

**Objetivo.** Completar web → ClickOnce → C# → Java → web → PDF.

**Arquitectura.** Flask conserva operaciones y genera PDF. Un enlace ClickOnce activa un lanzador C# que ejecuta el formulario Swing. Java devuelve un formulario HTTP UTF-8 y la web consulta el resultado.

**Tecnologías.** Python, Flask, ReportLab, Java 11, OpenJDK y C# para .NET Framework 4.8, con manifiestos generados por Microsoft.DotNet.Mage.

**Diseño aprobado.** [Especificación](../specs/2026-11-17-demo-clickonce-java-design.md).

**Estado.** Los componentes están implementados y compilan. El [estado de verificación](../../documentacion-tecnica.md#estado-de-verificación) distingue las comprobaciones completadas de las interacciones de escritorio pendientes.

## Restricciones

- Cada demo ocupa su propia carpeta dentro de `demo/`. Este caso completo pertenece a `demo/informacion-equipo/`.
- La carpeta raíz `lib/` queda destinada a la librería de Playwright Python.
- Tres campos editables en Java, con fecha actual, nombre del equipo y alias `Usuario demo`.
- Operaciones locales independientes con vigencia de veinte minutos.
- La confirmación devuelve los datos que aparecen en el PDF.
- El puerto predeterminado es `18765`; `start.ps1 -Port` configura conjuntamente la web y ClickOnce.

## Puntos de revisión

- Reenvíos concurrentes con datos distintos deben producir un único resultado.
- Una caída de comunicación debe permitir reintentar desde Java.
- Los errores de validación y cancelaciones deben ser visibles en la web.
- El lanzador debe recibir los parámetros al abrirse desde ClickOnce.
- Las pruebas parciales deben distinguirse de la activación real desde el navegador.

## Tarea 1 — web y operaciones

Archivos en `demo/informacion-equipo/web/`, con `app.py`, `operations.py`, `pdf_report.py`, `templates/`, `static/`, `requirements.txt` y `test_app.py`.

Contrato de los datos con nombres `date`, `computerName` y `alias`. `POST /api/operations` devuelve `operation_id` y `launch_url`. `GET /api/operations/<id>` devuelve `status` y los datos si existen. `POST /api/operations/<id>/result` confirma el formulario. Los endpoints `/cancel` y `/error` comunican el cierre y los fallos. `GET /api/operations/<id>/document.pdf` entrega el resultado.

- [x] Implementar y verificar aislamiento, validación, expiración, confirmaciones idempotentes y conflictos.
- [x] Generar el PDF y comprobar mediante extracción de texto que contiene los valores confirmados.
- [ ] Revisar visualmente el PDF en el navegador.
- [x] Crear la página con tres campos, **Obtener información**, estados y visor del PDF.

## Tarea 2 — formulario Java

Archivo principal `demo/informacion-equipo/desktop-java/src/InformationForm.java`. Recibe una URL de operación local, obtiene los valores iniciales y devuelve un formulario HTTP UTF-8. Las clases y el JAR se generan en `demo/informacion-equipo/desktop-java/build/`.

- [x] Implementar Swing con campos accesibles y botón **Confirmar**.
- [x] Implementar validación y envío fuera del hilo de UI, conservación de valores ante fallos y cancelación al cerrar.
- [x] Compilar para Java 11.
- [ ] Verificar en OpenJDK la edición, el envío, el reintento y el cierre de la ventana.

## Tarea 3 — lanzador y publicación

Archivos `demo/informacion-equipo/launcher-clickonce/Launcher.cs` y `demo/informacion-equipo/launcher-clickonce/build.ps1`. El lanzador usa `ApplicationDeployment.CurrentDeployment.ActivationUri`, valida el servidor local y ejecuta el JAR con el runtime configurado. La publicación contiene el lanzador, el JAR y la configuración Java en `launcher-clickonce/publish/`. Mage pertenece a `launcher-clickonce/.tools/` dentro de esta misma demo.

- [x] Compilar C# con las herramientas disponibles de Windows.
- [x] Generar manifiestos con parámetros URL habilitados y verificar su distribución desde Flask.
- [x] Corregir el conflicto de identidad Win32 con `/nowin32manifest`, recompilar y comprobar la ausencia de la identidad conflictiva en el ejecutable.
- [ ] Repetir la activación ClickOnce con la publicación corregida.
- [ ] Verificar la ejecución del JAR a través del lanzador.

## Tarea 4 — recorrido y documentación

- [x] Proporcionar `demo/informacion-equipo/start.ps1` para preparar dependencias, compilar los componentes y arrancar Flask con un solo comando. Se verificaron el arranque desde otra carpeta, el puerto configurable, HTTP 200, creación de operaciones, coherencia de la publicación, rechazo de puerto ocupado y parada con Ctrl+C. El arranque predeterminado quedó verificado en `18765`.
- [ ] Probar desde Edge la activación ClickOnce y observar Java.
- [ ] Editar los campos, confirmar y comprobar valores en la web y en el PDF.
- [x] Actualizar README con ejecución, compilación, configuración y archivos de la demo propia.
- [x] Mantener los detalles internos en la documentación técnica y crear `lib/README.md`.
- [ ] Ejecutar una revisión final independiente y corregir hallazgos antes de entregar.

La librería se desarrollará después de verificar el recorrido completo de la demo. Las comprobaciones parciales de compilación y backend no completan las tareas de interacción pendientes.
