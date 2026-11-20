$ErrorActionPreference = 'Stop'
$libraryRoot = Join-Path $PSScriptRoot '../../lib'

& (Join-Path $libraryRoot 'scripts/build_py.ps1')
$wheels = @(Get-ChildItem -LiteralPath (Join-Path $libraryRoot 'dist/python') -Filter '*.whl' -File)
if ($wheels.Count -ne 1) { throw 'Se esperaba un wheel en lib/dist/python.' }

$tempRoot = (Resolve-Path -LiteralPath $env:TEMP).Path
$localInstall = Join-Path $tempRoot ('pc-info-wheel-' + [guid]::NewGuid().ToString('N'))
$previousPythonPath = $env:PYTHONPATH
try {
    py -m pip install --target $localInstall $wheels[0].FullName pypdf
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo instalar el wheel local.' }

    $env:PYTHONPATH = $localInstall
    py (Join-Path $PSScriptRoot 'run.py')
    exit $LASTEXITCODE
}
finally {
    $env:PYTHONPATH = $previousPythonPath
    $directory = Get-Item -LiteralPath $localInstall -ErrorAction SilentlyContinue
    if ($directory -and $directory.FullName -eq $localInstall -and -not ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        Remove-Item -LiteralPath $directory.FullName -Recurse -Force
    }
}
