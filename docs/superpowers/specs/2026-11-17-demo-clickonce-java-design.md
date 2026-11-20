# Demo propia de intercambio de información

## Objetivo

`pc-info` demuestra cómo integrar Playwright con una aplicación de escritorio invocada desde la web. Las tareas y sus resultados se registran en el [plan de implementación](../plans/2026-11-17-demo-clickonce-java.md).

La demo enlaza una web Python, un lanzador ClickOnce en C# y una aplicación Java de escritorio. La web genera un PDF con los datos confirmados en Java y lo muestra en el visor del navegador.

## Requisitos del usuario

- Una web con tres campos y el botón **Obtener información**.
- Un JavaScript propio que invoque la aplicación mediante ClickOnce.
- Un lanzador C# que abra la aplicación Java.
- Una aplicación Java compatible con OpenJDK, con tres campos precargados y editables y el botón **Confirmar**.
- Los campos incluyen la fecha actual, un dato local del PC y un alias.
- Al confirmar en Java, los datos regresan a la web y se genera y previsualiza un PDF.
- Los componentes de la demo pertenecen a este repositorio.
- La aplicación está en `demo/pc-info/` y su automatización en `demo/pc-info-automation/`.

## Enfoque adoptado

Se activa ClickOnce una vez por operación y el resultado se devuelve directamente a Flask. Cada activación debe abrir una ventana Java asociada a esa operación.

## Campos y presentación

Los valores iniciales de esta versión son los siguientes.

| Campo | Nombre de intercambio | Valor inicial en Java |
| --- | --- | --- |
| Fecha | `date` | Fecha del equipo al abrir la ventana, en formato `YYYY-MM-DD`. |
| Nombre del equipo | `computerName` | Nombre del equipo obtenido localmente desde Java. |
| Alias | `alias` | `Usuario demo`. |

El nombre del equipo representa el dato obtenido fuera del navegador. Java consulta `COMPUTERNAME`, utiliza el hostname como alternativa y muestra `Equipo local` si ambas consultas fallan.

Todos los campos de Java permiten edición. La fecha debe ser válida en formato `YYYY-MM-DD` y los dos textos deben contener entre 1 y 100 caracteres visibles después de quitar espacios de los extremos. Los valores confirmados, incluidos los cambios realizados por el usuario, son los que deben recibir la web y el PDF.

La web muestra inicialmente vacíos los tres campos de solo lectura. Presenta los datos devueltos por Java. Durante una operación muestra el estado de espera y deshabilita **Obtener información**. Al completarse rellena el formulario y presenta el PDF. Una nueva operación vuelve a obtener los valores iniciales en Java y reemplaza el resultado visible de la anterior.

La ventana Java utiliza Swing, etiquetas asociadas a los campos y nombres accesibles estables. El título es **Información del equipo** y la acción principal es **Confirmar**.

## Componentes

| Componente | Responsabilidad |
| --- | --- |
| Web Python con Flask | Crear operaciones, recibir resultados, conservar su estado y generar y servir los PDF. |
| JavaScript propio | Crear la operación, solicitar la activación ClickOnce y consultar su resultado en Flask. |
| Lanzador C# | Obtener los parámetros de activación y ejecutar el JAR con el runtime Java configurado. Mostrar un error comprensible si falla el arranque. |
| Aplicación Java | Obtener los valores iniciales, presentar el formulario y enviar la confirmación o la cancelación al servidor. |
| Publicación ClickOnce | Distribuir el lanzador y el JAR mediante un manifiesto `.application` servido desde la demo. |

El formulario y la obtención de los datos del equipo pertenecen a Java. La generación del PDF pertenece a Python. El lanzador C# pasa la referencia de la operación al proceso Java y supervisa su arranque.

## Flujo

1. El usuario pulsa **Obtener información**.
2. Flask crea una operación con identificador aleatorio, estado `pending` y vigencia de veinte minutos.
3. JavaScript solicita la activación de la publicación ClickOnce con una referencia a esa operación en la URL.
4. ClickOnce ejecuta el lanzador C#, que inicia Java con los argumentos de la operación.
5. Java carga la fecha y el nombre del equipo localmente, precarga el alias y muestra el formulario editable.
6. **Confirmar** valida los valores y envía los tres campos al endpoint de resultado de Flask mediante un formulario HTTP UTF-8.
7. Flask valida los valores y genera el PDF. La operación pasa a `completed` cuando los datos y el PDF están disponibles.
8. La web consulta el estado periódicamente y, al completarse, rellena los campos y carga el PDF en un iframe.

Si falla la entrega, Java conserva la ventana y los valores y permite reintentar. Reenviar la misma confirmación produce el mismo resultado. Una confirmación con valores distintos para una operación ya completada se rechaza. Cerrar la ventana Java envía una cancelación si la operación continúa pendiente.

El servidor conserva separadas las operaciones para que varias pestañas o ventanas no intercambien sus datos. El identificador viaja con cada petición. Una operación vencida informa de su expiración. Los errores de validación mantienen la operación pendiente para que el usuario pueda corregir los valores.

La configuración inicial es local, con Flask en `127.0.0.1:18765` y puerto configurable. Las referencias de operación aceptadas por el lanzador y Java se limitan al servidor HTTP local de la demo. La comunicación depende de un identificador propio de operación y de servicios locales bajo nuestro control.

## Tecnologías y entorno

- Python global 3.10+ y Flask.
- ReportLab para generar el PDF y una fuente con soporte para los caracteres usados en los valores confirmados.
- Java Swing y APIs estándar compatibles con Java 11. La compilación utiliza `javac --release 11`.
- OpenJDK 11 o superior como runtime configurable mediante `-JavaHome`.
- C# como lanzador de Windows x64 para .NET Framework 4.8, compilado con `/nowin32manifest` para evitar un conflicto de identidades en ClickOnce.
- Microsoft.DotNet.Mage 10.0.0 para generar los manifiestos, instalado localmente mediante el SDK de .NET 9 o superior.
- Un navegador compatible con Playwright para ejecutar la automatización. El flujo completo requiere que ese navegador pueda activar ClickOnce.

`demo/pc-info/start.ps1` prepara las dependencias, compila los componentes, publica ClickOnce y arranca Flask en primer plano. Su parámetro `-Port` configura la misma dirección para la web y la publicación. Las instrucciones de uso están en el [README de demos](../../../demo/README.md).

## Alcance de la implementación

Todos los componentes de esta demo se implementan en `demo/pc-info/`, con sus pruebas y configuración. Se dividen en `web/`, `desktop-java/` y `launcher-clickonce/`. Las clases y el JAR se generan en `desktop-java/build/`. La publicación y Mage pertenecen a `launcher-clickonce/publish/` y `launcher-clickonce/.tools/`. El recorrido automatizado con la librería se ejecuta desde `demo/pc-info-automation/`.

`demo/README.md` centraliza el objetivo, contenido, ejecución y pruebas de las demos. El README principal presenta la librería y su API. Los detalles de arquitectura y del intercambio entre componentes pertenecen a la documentación técnica.

## Verificación requerida

- Crear dos operaciones y comprobar que cada una recibe exclusivamente su resultado.
- Editar los tres campos en Java y comprobar que sus valores coinciden en la web y en el PDF.
- Confirmar una operación, reintentar el mismo envío y rechazar un envío posterior con valores distintos.
- Comprobar corrección de fecha inválida, textos vacíos, cancelación, expiración y fallo de entrega.
- Ejecutar el JAR con OpenJDK y verificar que el lanzador abre ese proceso Java.
- Probar el recorrido desde el botón de la página hasta ClickOnce, la ventana Java, la confirmación y el PDF. Las comprobaciones que invoquen directamente el ejecutable se registrarán como verificaciones parciales.

## Referencias de implementación

- [Parámetros de activación ClickOnce en .NET Framework](https://learn.microsoft.com/en-us/visualstudio/deployment/how-to-retrieve-query-string-information-in-an-online-clickonce-application?view=visualstudio).
- [Publicación ClickOnce desde línea de comandos](https://learn.microsoft.com/en-us/visualstudio/deployment/building-dotnet-clickonce-applications-from-the-command-line?view=visualstudio).
