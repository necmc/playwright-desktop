param(
    [string]$BaseUrl = 'http://127.0.0.1:18765',
    [string]$JavaHome = $env:JAVA_HOME,
    [string]$JdkHome = $env:JAVA_HOME
)

$ErrorActionPreference = 'Stop'
$demoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$javaRoot = Join-Path $demoRoot 'desktop-java'
$buildRoot = Join-Path $javaRoot 'build'
$publishRoot = Join-Path $PSScriptRoot 'publish'
$buildTime = [DateTime]::UtcNow
$buildVersion = "1.0.$($buildTime.DayOfYear).$([int][Math]::Floor($buildTime.TimeOfDay.TotalSeconds / 2))"
$payloadRelative = "Application Files/InformationDemo_$($buildVersion.Replace('.', '_'))"
$payloadRoot = Join-Path $publishRoot $payloadRelative
$classRoot = Join-Path $buildRoot 'classes'
$toolRoot = Join-Path $PSScriptRoot '.tools'

if (-not $JdkHome -or -not (Test-Path -LiteralPath (Join-Path $JdkHome 'bin/javac.exe'))) {
    $compiler = Get-Command javac.exe -ErrorAction Stop
    $JdkHome = Split-Path (Split-Path $compiler.Source)
}
if (-not $JavaHome) {
    $openWebStartJava = 'D:/Program Files/OpenWebStart/jre'
    $JavaHome = if (Test-Path -LiteralPath (Join-Path $openWebStartJava 'bin/javaw.exe')) { $openWebStartJava } else { $JdkHome }
}
$JavaHome = [System.IO.Path]::GetFullPath($JavaHome)
if (-not (Test-Path -LiteralPath (Join-Path $JavaHome 'bin/javaw.exe'))) {
    throw 'JavaHome debe contener bin/javaw.exe.'
}
$publishUri = [Uri]$BaseUrl
if ($publishUri.Scheme -ne 'http' -or $publishUri.Host -notin @('127.0.0.1', 'localhost') -or $publishUri.AbsolutePath -ne '/') {
    throw 'BaseUrl debe ser la dirección HTTP local de la demo, por ejemplo http://127.0.0.1:18765.'
}

$applicationFilesRoot = Join-Path $publishRoot 'Application Files'
if (Test-Path -LiteralPath $applicationFilesRoot) {
    Remove-Item -LiteralPath $applicationFilesRoot -Recurse -Force
}
foreach ($directory in @($classRoot, $payloadRoot, $toolRoot)) {
    [void][System.IO.Directory]::CreateDirectory($directory)
}

& (Join-Path $JdkHome 'bin/javac.exe') --release 11 -encoding UTF-8 -d $classRoot (Join-Path $javaRoot 'src/InformationForm.java')
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación de Java.' }
$javaJar = Join-Path $buildRoot 'InformationForm.jar'
& (Join-Path $JdkHome 'bin/jar.exe') --create --file $javaJar --main-class InformationForm -C $classRoot .
if ($LASTEXITCODE -ne 0) { throw 'Falló la creación del JAR.' }
Copy-Item -LiteralPath $javaJar -Destination $payloadRoot

$csharp = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
# El manifiesto Win32 predeterminado de csc identifica MyApplication.app y
# entra en conflicto con la identidad administrada al validar ClickOnce.
& $csharp /nologo /target:winexe /platform:x64 /nowin32manifest "/out:$(Join-Path $payloadRoot 'InformationDemo.exe')" /reference:System.Deployment.dll /reference:System.Web.dll /reference:System.Configuration.dll /reference:System.Windows.Forms.dll (Join-Path $PSScriptRoot 'Launcher.cs')
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación del lanzador C#.' }

$escapedJavaHome = [System.Security.SecurityElement]::Escape($JavaHome)
$configuration = @"
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <startup><supportedRuntime version="v4.0" sku=".NETFramework,Version=v4.8" /></startup>
  <appSettings>
    <add key="JavaHome" value="$escapedJavaHome" />
  </appSettings>
</configuration>
"@
[System.IO.File]::WriteAllText((Join-Path $payloadRoot 'InformationDemo.exe.config'), $configuration, [System.Text.UTF8Encoding]::new($false))

$mage = Join-Path $toolRoot 'dotnet-mage.exe'
if (-not (Test-Path -LiteralPath $mage)) {
    dotnet tool install Microsoft.DotNet.Mage --version 10.0.0 --tool-path $toolRoot --allow-roll-forward
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo instalar Microsoft.DotNet.Mage.' }
}
$applicationManifest = Join-Path $payloadRoot 'InformationDemo.exe.manifest'
# Evitar incluir el manifiesto anterior dentro de sí mismo al recompilar.
if (Test-Path -LiteralPath $applicationManifest) { Remove-Item -LiteralPath $applicationManifest }
& $mage -New Application -ToFile $applicationManifest -Name InformationDemo -Version $buildVersion -Processor amd64 -FromDirectory $payloadRoot
if ($LASTEXITCODE -ne 0) { throw 'No se pudo generar el manifiesto de aplicación.' }
$deploymentManifest = Join-Path $publishRoot 'InformationDemo.application'
$providerUrl = $BaseUrl.TrimEnd('/') + '/clickonce/InformationDemo.application'
& $mage -New Deployment -ToFile $deploymentManifest -Name "Informaci$([char]0x00F3)n del equipo" -Version $buildVersion -Processor amd64 -AppManifest $applicationManifest -AppCodeBase "$payloadRelative/InformationDemo.exe.manifest" -ProviderUrl $providerUrl -Install false -TrustURLParameters true -Publisher 'playwright-desktop'
if ($LASTEXITCODE -ne 0) { throw 'No se pudo generar la publicación ClickOnce.' }

Write-Output "Publicación lista en $publishRoot"
Write-Output "Runtime Java configurado en $JavaHome"
