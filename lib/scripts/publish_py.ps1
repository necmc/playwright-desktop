[CmdletBinding(SupportsShouldProcess)]
param()

$ErrorActionPreference = 'Stop'
if (-not $PSCmdlet.ShouldProcess('playwright-desktop en PyPI', 'Compilar y publicar')) { return }
$previousToken = $env:MATURIN_PYPI_TOKEN
$previousRustFlags = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $env:MATURIN_PYPI_TOKEN = $null
    foreach ($line in Get-Content '../.env') {
        if ($line -match '^\s*(?:export\s+)?MATURIN_PYPI_TOKEN\s*=\s*(.+)$') {
            $env:MATURIN_PYPI_TOKEN = $Matches[1].Trim().Trim([char]34).Trim([char]39)
        }
    }
    if ($env:MATURIN_PYPI_TOKEN -notlike 'pypi-*' -or $env:MATURIN_PYPI_TOKEN -match 'REEMPLAZAR') {
        throw 'Configura tu token de PyPI en MATURIN_PYPI_TOKEN dentro de .env.'
    }
    if (Test-Path '.tools/cargo/bin/cargo.exe') {
        $env:CARGO_HOME = Join-Path $PWD '.tools/cargo'
        $env:RUSTUP_HOME = Join-Path $PWD '.tools/rustup'
        $env:PATH = "$env:CARGO_HOME/bin;$env:PATH"
    }
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE '.cargo' }
    $rustFlags = if ($previousRustFlags) { $previousRustFlags -split [char]31 } else { $env:RUSTFLAGS -split '\s+' | Where-Object { $_ } }
    $rustFlags = @($rustFlags) + "--remap-path-prefix=$PWD=/src" + "--remap-path-prefix=$cargoHome=/cargo"
    $env:CARGO_ENCODED_RUSTFLAGS = $rustFlags -join [char]31
    py -B -m maturin publish --locked --no-sdist --non-interactive --target-dir rust/target/python --out dist/python
    if ($LASTEXITCODE -ne 0) { throw 'Falló la publicación en PyPI.' }
}
finally {
    $env:MATURIN_PYPI_TOKEN = $previousToken
    $env:CARGO_ENCODED_RUSTFLAGS = $previousRustFlags
    Pop-Location
}
