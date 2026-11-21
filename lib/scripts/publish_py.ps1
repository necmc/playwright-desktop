[CmdletBinding(SupportsShouldProcess)]
param([switch]$SkipBuild)

$ErrorActionPreference = 'Stop'
if (-not $PSCmdlet.ShouldProcess('playwright-desktop en PyPI', 'Compilar y publicar')) { return }
$previousUsername = $env:TWINE_USERNAME
$previousPassword = $env:TWINE_PASSWORD
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $pythonCmd = Get-Command python -ErrorAction SilentlyContinue
    $python = if ($pythonCmd -and $pythonCmd.Source -notlike '*\WindowsApps\python.exe') { $pythonCmd.Source } else { 'py' }

    $token = if ($env:PYPI_TOKEN) {
        $env:PYPI_TOKEN.Trim().Trim([char]34).Trim([char]39)
    }
    else {
        $null
    }
    if (-not $token -and (Test-Path -LiteralPath '../.env')) {
        foreach ($line in Get-Content '../.env') {
            if ($line -match '^\s*(?:export\s+)?PYPI_TOKEN\s*=\s*(.+)$') {
                $token = $Matches[1].Trim().Trim([char]34).Trim([char]39)
            }
        }
    }
    if (-not $token -or $token -notlike 'pypi-*' -or $token -match 'REEMPLAZAR') {
        throw 'Token de publicación de PyPI no configurado o inválido.'
    }
    & $python -B -m twine --version > $null
    if ($LASTEXITCODE -ne 0) { throw 'Instala Twine con: pip install twine' }
    if (-not $SkipBuild) {
        & (Join-Path $PSScriptRoot 'build_py.ps1')
    }
    $wheels = @(Get-ChildItem -Path 'dist/python/*.whl' -ErrorAction SilentlyContinue)
    $sdists = @(Get-ChildItem -Path 'dist/python/*.tar.gz' -ErrorAction SilentlyContinue)
    if (-not $wheels -or -not $sdists) {
        throw 'Se requieren archivos .whl y .tar.gz en dist/python para publicar.'
    }
    $artifacts = @($wheels + $sdists | ForEach-Object { $_.FullName })
    $env:TWINE_USERNAME = '__token__'
    $env:TWINE_PASSWORD = $token
    & $python -B -m twine upload --non-interactive @artifacts
    if ($LASTEXITCODE -ne 0) { throw 'Falló la publicación en PyPI.' }
}
finally {
    $env:TWINE_USERNAME = $previousUsername
    $env:TWINE_PASSWORD = $previousPassword
    Pop-Location
}
