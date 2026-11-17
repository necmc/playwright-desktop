[CmdletBinding()]
param(
    [ValidateRange(1, 65535)]
    [int]$Port = 18765,
    [string]$JavaHome = $env:JAVA_HOME,
    [string]$JdkHome = $env:JAVA_HOME,
    [string]$Python = 'py',
    [switch]$EnableAccessBridge
)

$ErrorActionPreference = 'Stop'
$baseUrl = "http://127.0.0.1:$Port"
$webRoot = Join-Path $PSScriptRoot 'web'
$buildScript = Join-Path $PSScriptRoot 'launcher-clickonce/build.ps1'

# Fallar antes de instalar o compilar si ya hay otro servidor en este puerto.
$listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
try {
    $listener.Server.ExclusiveAddressUse = $true
    $listener.Start()
}
catch {
    throw "No se puede utilizar el puerto $Port. Detén el servidor que lo ocupa o ejecuta este script con -Port 18766. Detalle: $($_.Exception.Message)"
}
finally {
    $listener.Stop()
}

if (-not (Get-Command $Python -ErrorAction SilentlyContinue)) {
    throw 'No se encontró Python. Instala Python 3.10+ o indica su ejecutable con -Python.'
}
& $Python -c 'import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)'
if ($LASTEXITCODE -ne 0) { throw 'Se requiere Python 3.10 o superior.' }

Write-Host '[1/3] Preparando las dependencias de Python global...'
& $Python -m pip install --user --disable-pip-version-check -r (Join-Path $webRoot 'requirements.txt')
if ($LASTEXITCODE -ne 0) { throw 'No se pudieron instalar las dependencias Python de la demo.' }

Write-Host '[2/3] Compilando Java, el lanzador C# y la publicación ClickOnce...'
& $buildScript -BaseUrl $baseUrl -JavaHome $JavaHome -JdkHome $JdkHome -EnableAccessBridge:$EnableAccessBridge

Write-Host "[3/3] Arrancando la web en $baseUrl"
Write-Host 'Abre esta dirección en Microsoft Edge y pulsa Obtener información.'
Write-Host 'Mantén esta terminal abierta. Usa Ctrl+C para detener el servidor.'

# Un único proceso de servidor en primer plano facilita detener la demo.
& $Python -B -m flask --app (Join-Path $webRoot 'app.py') run --host 127.0.0.1 --port $Port --no-reload
if ($LASTEXITCODE -ne 0) { throw 'El servidor web terminó con un error.' }
