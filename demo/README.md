# Demo de playwright-desktop

`pc-info` abre un formulario de escritorio desde el navegador y genera un PDF con los datos que confirmas. `pc-info-automation` verifica ese recorrido con `playwright-desktop`.

Ejecuta los comandos en PowerShell desde la raíz del repositorio. Necesitas Windows x64, Python 3.10–3.14, JDK 11 o posterior, .NET Framework 4.8, SDK de .NET 9 o posterior y un navegador con soporte para ClickOnce. Microsoft Edge es la opción recomendada. Para otro navegador, instala una extensión compatible si hace falta.

## Levantar pc-info

```powershell
.\demo\pc-info\start.ps1 -EnableAccessBridge
```

Abre [http://127.0.0.1:18765](http://127.0.0.1:18765) y pulsa **Obtener información**. Si el formulario no se abre automáticamente, usa **Abrir aplicación** y acepta la solicitud de ClickOnce si aparece. Edita los datos y pulsa **Confirmar** para ver el PDF en la web.

## Ejecutar pc-info-automation

Deja `pc-info` en ejecución con el comando anterior. En otra terminal, desde la raíz, instala las dependencias de la automatización una sola vez:

```powershell
py -m pip install playwright-desktop pypdf
```

Ejecuta la automatización:

```powershell
py demo/pc-info-automation/run.py
```

La automatización usa tu navegador predeterminado si es compatible con Playwright. Si solicita abrir ClickOnce, confirma la apertura. Para usar una extensión, instálala en el [perfil de pruebas](../docs/documentacion-tecnica.md#navegador-de-la-automatización).

[Plan de pruebas y estructura POM](pc-info-automation/test_plan.md).
