# pc-info — plan de implementación

**Objetivo.** Completar web → ClickOnce → C# → Java → web → PDF.

**Arquitectura.** Flask conserva operaciones y genera PDF. Un enlace ClickOnce activa un lanzador C# que ejecuta el formulario Swing. Java devuelve un formulario HTTP UTF-8 y la web consulta el resultado.

**Tecnologías.** Python, Flask, ReportLab, Java 11, OpenJDK y C# para .NET Framework 4.8, con manifiestos generados por Microsoft.DotNet.Mage.

**Especificación.** [Diseño de pc-info](../specs/2026-11-17-demo-clickonce-java-design.md).

**Estado.** En validación. La web, el formulario y el lanzador están implementados. Faltan la prueba de reintento y la revisión final. El recorrido completo de CP-PI-01 se sigue en el [plan de activación ClickOnce](2026-11-19-pc-info-activacion-clickonce.md).

## Restricciones globales

- La aplicación de ejemplo está en `demo/pc-info/` y su automatización en `demo/pc-info-automation/`.
- Tres campos editables en Java, con fecha actual, nombre del equipo y alias `Usuario demo`.
- Operaciones locales independientes con vigencia de veinte minutos.
- La confirmación devuelve los datos que aparecen en el PDF.
- El puerto predeterminado es `18765`; `start.ps1 -Port` configura conjuntamente la web y ClickOnce.

## Foco de revisión

- Reenvíos concurrentes con datos distintos deben producir un único resultado.
- Una caída de comunicación debe permitir reintentar desde Java.
- Los errores de validación y cancelaciones deben ser visibles en la web.
- El lanzador debe recibir los parámetros al abrirse desde ClickOnce.
- Las pruebas parciales deben distinguirse de la activación real desde el navegador.

## Tarea 1 — web y operaciones

Archivos en `demo/pc-info/web/`, con `app.py`, `operations.py`, `pdf_report.py`, `templates/`, `static/`, `requirements.txt` y `test_app.py`.

Contrato de los datos con nombres `date`, `computerName` y `alias`. `POST /api/operations` devuelve `operation_id` y `launch_url`. `GET /api/operations/<id>` devuelve `status` y los datos si existen. `POST /api/operations/<id>/result` confirma el formulario. Los endpoints `/cancel` y `/error` comunican el cierre y los fallos. `GET /api/operations/<id>/document.pdf` entrega el resultado.

- [x] Implementar y verificar el contrato HTTP. Las ocho pruebas del backend cubren aislamiento, concurrencia, validación, expiración, reintentos y conflictos.
- [x] Generar el PDF y comprobar mediante extracción de texto que contiene los valores confirmados.
- [x] Crear la página con tres campos, **Obtener información**, estados y visor del PDF.

## Tarea 2 — formulario Java

Archivo principal `demo/pc-info/desktop-java/src/InformationForm.java`. Recibe una URL de operación local, obtiene los valores iniciales y devuelve un formulario HTTP UTF-8. Las clases y el JAR se generan en `demo/pc-info/desktop-java/build/`.

- [x] Implementar Swing con campos accesibles y botón **Confirmar**.
- [x] Implementar validación y envío fuera del hilo de UI, conservación de valores ante fallos y cancelación al cerrar.
- [x] Compilar para Java 11.
- [x] Verificar con OpenJDK 11 el formulario en arranque directo.
- [ ] Reproducir un fallo de entrega en Java y comprobar que el reintento conserva los valores y completa la operación.

## Tarea 3 — lanzador y publicación

Archivos `demo/pc-info/launcher-clickonce/Launcher.cs` y `demo/pc-info/launcher-clickonce/build.ps1`. El lanzador usa `ApplicationDeployment.CurrentDeployment.ActivationUri`, valida el servidor local y ejecuta el JAR con el runtime configurado. La publicación contiene el lanzador, el JAR y la configuración Java en `launcher-clickonce/publish/`. Mage pertenece a `launcher-clickonce/.tools/` dentro de esta misma demo.

- [x] Compilar C# con las herramientas disponibles de Windows.
- [x] Generar manifiestos con parámetros URL habilitados y verificar su distribución desde Flask.
- [x] Compilar con `/nowin32manifest` para conservar la identidad declarada en los manifiestos ClickOnce.
- [x] Comprobar el arranque del lanzador y del JAR mediante el controlador ClickOnce del sistema fuera del navegador.

La [tarea de activación completa](2026-11-19-pc-info-activacion-clickonce.md#tarea-2--activación-completa) centraliza la verificación desde el navegador.

## Tarea 4 — recorrido y documentación

- [x] Proporcionar `demo/pc-info/start.ps1` para preparar dependencias, compilar los componentes y arrancar Flask con un solo comando. Se verificaron el arranque desde otra carpeta, el puerto configurable, HTTP 200, creación de operaciones, coherencia de la publicación, rechazo de puerto ocupado y parada con Ctrl+C. El arranque predeterminado quedó verificado en `18765`.
- [x] Documentar el arranque y la ejecución de la automatización en `demo/README.md`.
- [x] Documentar el intercambio entre los componentes en la [documentación técnica](../../documentacion-tecnica.md).
- [x] Verificar el clic de CP-PI-02 en la bandeja de descargas de Edge. Las comprobaciones por interfaz y navegador se registran en el [plan de activación ClickOnce](2026-11-19-pc-info-activacion-clickonce.md#tarea-1--clic-de-apertura-desde-el-navegador).
- [ ] Ejecutar una revisión final independiente de la demo y corregir los hallazgos antes de entregar. Usar la evidencia de CP-PI-01.
