# Demos de playwright-desktop

La demo `pc-info` abre un formulario Java desde la web mediante ClickOnce y genera un PDF con los datos confirmados. `pc-info-automation` automatiza ese recorrido con `playwright-desktop` y Page Object Model (POM).

Ejecuta los comandos desde la raíz del repositorio, en Windows y PowerShell. Necesitas Python, JDK/OpenJDK 11+, .NET Framework 4.8, SDK .NET 9+ y Edge.

## Levantar pc-info

La aplicación está en [pc-info/](pc-info/): web Python, formulario Java y lanzador ClickOnce.

```powershell
.\demo\pc-info\start.ps1 -EnableAccessBridge
```

El script prepara los componentes y arranca la web en [http://127.0.0.1:18765](http://127.0.0.1:18765). Mantén la terminal abierta.

Abre la dirección en Edge, pulsa **Obtener información**, edita los datos en Java y pulsa **Confirmar** para ver el PDF. Acepta la apertura de ClickOnce si Edge la solicita. Usa `Ctrl+C` para detener la web.

## Ejecutar pc-info-automation

El código está en [pc-info-automation/](pc-info-automation/): `tests/` contiene el escenario, `pages/` los Page Objects y `support/` la preparación y limpieza.

El [escenario](pc-info-automation/tests/test_information_flow.py) muestra `import playwright_desktop`, `page.desktop.configure()` y `page.desktop.expect_window()`. El [Page Object de Java](pc-info-automation/pages/information_form.py) utiliza `page.desktop.window()` y sus controles con `get_by_role()`, `fill()` y `click()`.

Con el escenario preparado, detén la web manual e instala las dependencias:

```shell
pip install playwright-desktop pypdf
```

Ejecuta la automatización:

```powershell
py demo/pc-info-automation/run.py
```

El comando inicia la web y Edge, ejecuta el escenario y verifica los datos de la página y el PDF. Al terminar cierra sus recursos y elimina el perfil temporal del navegador.
