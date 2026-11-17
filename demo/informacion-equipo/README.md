# Demo de información del equipo

Una web Python abre mediante ClickOnce un lanzador C# que ejecuta un formulario Java. El formulario precarga la fecha actual, el nombre del equipo y el alias `Usuario demo`. Al pulsar **Confirmar**, devuelve los valores a la web para generar y previsualizar un PDF.

Esta demo proporciona el caso de integración de `playwright-desktop`. La automatización mediante `page.desktop` está definida en el diseño de la librería y pendiente de implementación.

## Requisitos

- Windows x64 y PowerShell.
- Python 3.10+ global con pip.
- JDK 11+ para compilar y OpenJDK 11+ para ejecutar Java.
- .NET Framework 4.8 y un SDK de .NET 9 o superior para instalar y ejecutar Mage.
- Microsoft Edge para probar la activación ClickOnce.

La primera preparación necesita acceso a PyPI y NuGet para descargar las dependencias que falten.

## Preparar y ejecutar

Desde la raíz del repositorio, en PowerShell:

```powershell
.\demo\informacion-equipo\start.ps1
```

[start.ps1](start.ps1) instala las dependencias de Python global, compila el JAR y el lanzador C#, genera la publicación ClickOnce y arranca Flask. Mantén esa terminal abierta. Usa `Ctrl+C` para detener la web.

Abre en Edge la dirección que muestre la terminal; por defecto, [http://127.0.0.1:18765](http://127.0.0.1:18765). El servidor se ejecuta sin recarga automática: reinicia el script después de cambiar el código.

## Configuración

El script configura el mismo puerto para Flask y la publicación ClickOnce. Si el puerto está ocupado, detén el servidor anterior o elige otro:

```powershell
.\demo\informacion-equipo\start.ps1 -Port 18766
```

Puedes indicar las rutas de tu instalación:

```powershell
.\demo\informacion-equipo\start.ps1 -JdkHome 'C:\ruta\jdk' -JavaHome 'C:\ruta\openjdk' -Python 'C:\ruta\python.exe'
```

Por defecto se usa `py` para Python. Para compilar Java, se utiliza `JAVA_HOME` o se busca `javac` en el `PATH`. Para ejecutar, se utiliza `JAVA_HOME`; si no está definido, el script busca OpenWebStart en la ruta local configurada y, en su defecto, utiliza el JDK. Indica `-JavaHome` para seleccionar explícitamente OpenJDK.

Cada ejecución recompila los componentes y genera la publicación. Las operaciones se conservan en memoria durante veinte minutos y se pierden al detener el servidor.

## Prueba manual

1. Arranca la demo y abre la web en Edge.
2. Pulsa **Obtener información** y atiende la solicitud de apertura de ClickOnce.
3. En la ventana Java, modifica fecha, nombre del equipo y alias.
4. Pulsa **Confirmar**.
5. Comprueba que Java se cierre, los tres valores aparezcan en la web y coincidan en el PDF previsualizado.

La compilación, el arranque HTTP y las pruebas del backend están verificados. Falta repetir el recorrido completo tras la corrección del manifiesto ClickOnce. El [estado de verificación](../../docs/documentacion-tecnica.md#estado-de-verificación) detalla las comprobaciones realizadas y pendientes.

## Pruebas del backend

Desde la raíz del repositorio:

```powershell
py -m pip install --user -r demo/informacion-equipo/web/requirements-test.txt
py -m unittest discover -s demo/informacion-equipo/web -p test_app.py -v
```

Las ocho pruebas comprueban aislamiento entre operaciones, validación, expiración, cancelación, reintentos, concurrencia y contenido textual del PDF. No realizan la apertura real de ClickOnce ni controlan la ventana Java.

## Archivos

```text
start.ps1                     Preparación y arranque de la demo
web/
  app.py                      Servidor Flask y endpoints
  operations.py               Estado y validación de operaciones
  pdf_report.py               Generación del PDF
  templates/                  Página web
  static/                     Estilos y JavaScript de activación ClickOnce
  test_app.py                 Pruebas del backend
  requirements*.txt           Dependencias de ejecución y pruebas
desktop-java/
  src/                        Formulario Java
  build/                      Clases y JAR compilados
launcher-clickonce/
  Launcher.cs                 Lanzador C#
  build.ps1                   Compilación y publicación
  publish/                    Manifiestos y archivos para ClickOnce
  .tools/                     Mage instalado por el script
```

`build/`, `publish/` y `.tools/` se generan dentro de su componente y están excluidos de Git. La [documentación técnica](../../docs/documentacion-tecnica.md) describe el intercambio entre componentes y la corrección del manifiesto ClickOnce.

[Volver a las demos](../README.md) · [Volver a la librería](../../README.md).
