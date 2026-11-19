# Demo de playwright-desktop

Ejecuta los comandos en PowerShell desde la raíz del repositorio.

## Levantar pc-info

Requiere Windows x64, Python 3.10–3.14, JDK 11+, .NET Framework 4.8, SDK de .NET 9+ y Edge u otro navegador compatible con ClickOnce.

```powershell
.\demo\pc-info\start.ps1
```

En [http://127.0.0.1:18765](http://127.0.0.1:18765), pulsa **Obtener información**. Usa **Abrir aplicación** si aparece. Confirma los datos en Java para ver el PDF.

## Ejecutar pc-info-automation

Deja `pc-info` en ejecución y abre otra terminal.

### Versión publicada

```powershell
py -m pip install playwright-desktop pypdf
py demo/pc-info-automation/run.py
```

### Versión local

Requiere Maturin, Rust MSVC y las herramientas C++ de Visual Studio.

```powershell
.\demo\pc-info-automation\run_local.ps1
```
