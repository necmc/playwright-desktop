# Demo de playwright-desktop

Ejecuta los comandos en PowerShell desde la raíz del repositorio.

## Levantar pc-info

Requiere Windows x64, Python 3.10–3.14, JDK 11+, .NET Framework 4.8, SDK de .NET 9+ y Edge u otro navegador compatible con ClickOnce.

```powershell
.\demo\pc-info\start.ps1
```

Abre la demo y pulsa **Obtener información**. Confirma el aviso del navegador o usa el enlace **Abrir aplicación** si el aviso no aparece. Confirma los datos en Java para ver el PDF. [Abrir pc-info](http://127.0.0.1:18765).

## Ejecutar pc-info-automation

Instala Microsoft Edge, deja `pc-info` en ejecución y abre otra terminal. Define `JAVA_HOME` con el JDK de la demo. Ambas opciones ejecutan el mismo recorrido. Si Edge pide confirmación, pulsa **Abrir** cuando la prueba lo indique.

### Versión publicada

```powershell
py -m pip install playwright-desktop pypdf
py demo/pc-info-automation/run.py
```

### Versión local

Usa esta opción para validar cambios en desarrollo. Requiere Maturin, Rust MSVC y las herramientas C++ de Visual Studio.

```powershell
.\demo\pc-info-automation\run_local.ps1
```
