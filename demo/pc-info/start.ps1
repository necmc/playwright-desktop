[CmdletBinding()]
param(
    [ValidateRange(1, 65535)]
    [int]$Port = 18765,
    [string]$JavaHome = $env:JAVA_HOME,
    [string]$JdkHome = $env:JAVA_HOME,
    [string]$Python = 'py'
)

$ErrorActionPreference = 'Stop'
$DefaultHost = '127.0.0.1'
$MinPythonMajor = 3
$MinPythonMinor = 10
$YesConfirmationPattern = "^(s|si|s$([char]0x00ED)|y|yes)$"
$WebRelativePath = 'web'
$FlaskAppFile = 'app.py'
$RequirementsFile = 'requirements.txt'
$BuildScriptRelativePath = 'launcher-clickonce/build.ps1'

$baseUrl = "http://${DefaultHost}:$Port"
$webRoot = Join-Path $PSScriptRoot $WebRelativePath
$buildScript = Join-Path $PSScriptRoot $BuildScriptRelativePath

# Si el puerto está ocupado, preguntar antes de liberarlo o detener la ejecución.
$owners = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | Select-Object -ExpandProperty OwningProcess -Unique)
if ($owners.Count -gt 0) {
    $answer = Read-Host "El puerto $Port está ocupado (PID $($owners -join ', ')). ¿Deseas liberar el puerto? [s/N]"
    if (([string]$answer).Trim() -notmatch $YesConfirmationPattern) {
        throw "No se puede utilizar el puerto $Port porque sigue ocupado."
    }
    try {
        Stop-Process -Id $owners -Force -ErrorAction Stop
    }
    catch {
        throw "No se pudo liberar el puerto $Port. Detalle: $($_.Exception.Message)"
    }
    Write-Host "Puerto $Port liberado."
}

if (-not (Get-Command $Python -ErrorAction SilentlyContinue)) {
    throw 'No se encontró Python. Instala Python 3.10+ o indica su ejecutable con -Python.'
}
& $Python -c "import sys; sys.exit(0 if sys.version_info >= ($MinPythonMajor, $MinPythonMinor) else 1)"
if ($LASTEXITCODE -ne 0) { throw "Se requiere Python $MinPythonMajor.$MinPythonMinor o superior." }

Write-Host '[1/3] Preparando las dependencias de Python global...'
& $Python -m pip install --user --disable-pip-version-check -r (Join-Path $webRoot $RequirementsFile)
if ($LASTEXITCODE -ne 0) { throw 'No se pudieron instalar las dependencias Python de la demo.' }

Write-Host '[2/3] Compilando Java, el lanzador C# y la publicación ClickOnce...'
& $buildScript -BaseUrl $baseUrl -JavaHome $JavaHome -JdkHome $JdkHome

Write-Host "[3/3] Arrancando la web en $baseUrl"
Write-Host 'Abre esta dirección en tu navegador y pulsa Obtener información.'
Write-Host 'Mantén esta terminal abierta. Usa Ctrl+C para detener el servidor.'

# Un único proceso de servidor en primer plano facilita detener la demo.
& $Python -B -m flask --app (Join-Path $webRoot $FlaskAppFile) run --host $DefaultHost --port $Port --no-reload
if ($LASTEXITCODE -ne 0) { throw 'El servidor web terminó con un error.' }
