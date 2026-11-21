$ErrorActionPreference = 'Stop'
$libraryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$previousRustFlags = $env:CARGO_ENCODED_RUSTFLAGS
$previousCargoHome = $env:CARGO_HOME
$previousRustupHome = $env:RUSTUP_HOME
$previousPath = $env:PATH
Push-Location $libraryRoot
try {
    foreach ($directory in @('rust/target/python', 'dist/python')) {
        $buildPath = [IO.Path]::GetFullPath((Join-Path $libraryRoot $directory))
        if (-not $buildPath.StartsWith($libraryRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'La ruta de limpieza sale de la librería.'
        }
        if (Test-Path -LiteralPath $buildPath) {
            Remove-Item -LiteralPath $buildPath -Recurse -Force
        }
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
    $pythonCmd = Get-Command python -ErrorAction SilentlyContinue
    $python = if ($pythonCmd -and $pythonCmd.Source -notlike '*\WindowsApps\python.exe') { $pythonCmd.Source } else { 'py' }
    & $python -B -m maturin build --release --locked --target-dir rust/target/python --out dist/python
    if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación del paquete Python.' }
    & $python -B -m maturin sdist --manifest-path rust/Cargo.toml --out dist/python
    if ($LASTEXITCODE -ne 0) { throw 'Falló la generación del paquete fuente (.tar.gz).' }
}
finally {
    $env:PATH = $previousPath
    $env:RUSTUP_HOME = $previousRustupHome
    $env:CARGO_HOME = $previousCargoHome
    $env:CARGO_ENCODED_RUSTFLAGS = $previousRustFlags
    Pop-Location
}
