# QA Automation Interop

Proyecto para crear una librería de Playwright Python que integre pruebas web con aplicaciones de escritorio invocadas desde el navegador.

Actualmente estamos preparando la demo. `lib/` contiene únicamente su README; la librería todavía no está implementada.

## Demo de información del equipo

La demo incluye una web Python, un lanzador C# con ClickOnce y un formulario Java. El flujo previsto es pulsar **Obtener información**, editar la fecha, el nombre del equipo y el alias en Java y pulsar **Confirmar** para devolverlos a la web y previsualizarlos en un PDF.

Se verificaron la compilación, el arranque del servidor y las pruebas del backend. Falta validar el recorrido completo desde el navegador hasta Java y el PDF tras la corrección del manifiesto ClickOnce. El [estado de verificación](docs/documentacion-tecnica.md#estado-de-verificación) detalla el alcance comprobado.

### Preparar y ejecutar

Requiere Windows x64, PowerShell, Python 3.10+ con pip, un JDK 11+ para compilar, OpenJDK 11+ para ejecutar, .NET Framework 4.8 y un SDK de .NET 9 o superior para instalar y ejecutar Mage.

Desde la raíz del repositorio, en PowerShell:

```powershell
.\demo\informacion-equipo\start.ps1
```

El script [start.ps1](demo/informacion-equipo/start.ps1) instala las dependencias de Python global, compila el JAR y el lanzador C#, genera la publicación ClickOnce y arranca Flask. La primera preparación necesita acceso a PyPI y NuGet para descargar las dependencias que falten. Mantén la terminal abierta y usa `Ctrl+C` para detener la web.

Para probarla, abre en Microsoft Edge la dirección que muestre la terminal; por defecto, [http://127.0.0.1:18765](http://127.0.0.1:18765). Pulsa **Obtener información** y atiende la solicitud de apertura de ClickOnce. Los tres campos de la web muestran los datos devueltos por Java.

Java precarga la fecha actual, el nombre del equipo y el alias `Usuario demo`. Las operaciones caducan a los veinte minutos y se pierden al detener el servidor.

### Configuración

El script configura el mismo puerto para la web y ClickOnce. Si el puerto está ocupado, detén el servidor anterior o elige otro:

```powershell
.\demo\informacion-equipo\start.ps1 -Port 18766
```

Puedes indicar `-JdkHome 'C:\ruta\jdk'`, `-JavaHome 'C:\ruta\openjdk'` y `-Python 'C:\ruta\python.exe'` según tu instalación. Por defecto se usa `py` para Python. Para compilar Java, se usa `JAVA_HOME` o se busca `javac` en el `PATH`. Para ejecutar, se usa `JAVA_HOME`; si no está definido, el script busca OpenWebStart en la ruta local configurada y, en su defecto, utiliza el JDK. Indica `-JavaHome` para seleccionar explícitamente OpenJDK.

Cada ejecución recompila los componentes y genera una versión ClickOnce nueva. Reinicia el script después de cambiar el código.

### Carpetas

```text
lib/                              Reservada para la futura librería
demo/
  informacion-equipo/             Demo de consulta del equipo
    start.ps1                     Preparar todo y arrancar la demo manual
    web/                          Flask, formulario, PDF y pruebas
      templates/                  Página web
      static/                     Estilos y JS de activación ClickOnce
    desktop-java/
      src/                        Formulario Java
      build/                      Clases y JAR compilados
    launcher-clickonce/
      Launcher.cs                 Lanzador C#
      build.ps1                   Compilación de Java y publicación ClickOnce
      publish/                    Manifiestos y archivos para ClickOnce
      .tools/                     Mage instalado por el script
```

Cada nueva demo tendrá su propia carpeta en `demo/`. `build/`, `publish/` y `.tools/` son archivos generados, excluidos de Git y contenidos en el componente correspondiente.

### Pruebas del backend

```powershell
py -m pip install --user -r demo/informacion-equipo/web/requirements-test.txt
py -m unittest discover -s demo/informacion-equipo/web -p test_app.py -v
```

## Documentación técnica

La [documentación técnica](docs/documentacion-tecnica.md) explica la arquitectura y el intercambio entre componentes.
