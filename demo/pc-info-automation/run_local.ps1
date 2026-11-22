$ErrorActionPreference = 'Stop'
$LibraryRelativePath = '../../lib'
$DistRelativePath = 'dist/python'
$BuildScriptRelativePath = 'scripts/build_py.ps1'
$WheelFilter = '*.whl'
$TempPrefix = 'pc-info-wheel-'
$ExtraPackages = @('pypdf')
$RunnerScript = 'run.py'

$libraryRoot = Join-Path $PSScriptRoot $LibraryRelativePath

& (Join-Path $libraryRoot $BuildScriptRelativePath)
$wheels = @(Get-ChildItem -LiteralPath (Join-Path $libraryRoot $DistRelativePath) -Filter $WheelFilter -File)
if ($wheels.Count -ne 1) { throw 'Se esperaba un wheel en lib/dist/python.' }

$tempRoot = (Resolve-Path -LiteralPath $env:TEMP).Path
$localInstall = Join-Path $tempRoot ($TempPrefix + [guid]::NewGuid().ToString('N'))
$previousPythonPath = $env:PYTHONPATH
try {
    py -m pip install --target $localInstall $wheels[0].FullName @ExtraPackages
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo instalar el wheel local.' }

    $env:PYTHONPATH = $localInstall
    py (Join-Path $PSScriptRoot $RunnerScript)
    exit $LASTEXITCODE
}
finally {
    $env:PYTHONPATH = $previousPythonPath
    $directory = Get-Item -LiteralPath $localInstall -ErrorAction SilentlyContinue
    if ($directory -and $directory.FullName -eq $localInstall -and -not ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        Remove-Item -LiteralPath $directory.FullName -Recurse -Force
    }
}
