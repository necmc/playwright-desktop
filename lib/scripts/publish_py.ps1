[CmdletBinding(SupportsShouldProcess)]
param()

$ErrorActionPreference = 'Stop'
if (-not $PSCmdlet.ShouldProcess('playwright-desktop en PyPI', 'Compilar y publicar')) { return }
$previousUsername = $env:TWINE_USERNAME
$previousPassword = $env:TWINE_PASSWORD
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $token = $null
    foreach ($line in Get-Content '../.env') {
        if ($line -match '^\s*(?:export\s+)?MATURIN_PYPI_TOKEN\s*=\s*(.+)$') {
            $token = $Matches[1].Trim().Trim([char]34).Trim([char]39)
        }
    }
    if ($token -notlike 'pypi-*' -or $token -match 'REEMPLAZAR') {
        throw 'Configura tu token de PyPI en MATURIN_PYPI_TOKEN dentro de .env.'
    }
    py -B -m twine --version > $null
    if ($LASTEXITCODE -ne 0) { throw 'Instala Twine con: pip install twine' }
    & (Join-Path $PSScriptRoot 'build_py.ps1')
    $env:TWINE_USERNAME = '__token__'
    $env:TWINE_PASSWORD = $token
    py -B -m twine upload --non-interactive dist/python/*.whl
    if ($LASTEXITCODE -ne 0) { throw 'Falló la publicación en PyPI.' }
}
finally {
    $env:TWINE_USERNAME = $previousUsername
    $env:TWINE_PASSWORD = $previousPassword
    Pop-Location
}
