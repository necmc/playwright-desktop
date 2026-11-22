param(
    [string]$BaseUrl = 'http://127.0.0.1:18765',
    [string]$JavaHome = $env:JAVA_HOME,
    [string]$JdkHome = $env:JAVA_HOME
)

$ErrorActionPreference = 'Stop'
$JavaReleaseVersion = '11'
$MainClassName = 'InformationForm'
$JavaSourceRelativePath = 'src/InformationForm.java'
$JarFileName = 'InformationForm.jar'
$LauncherSourceFile = 'Launcher.cs'
$LauncherExeName = 'InformationDemo.exe'
$AppManifestName = 'InformationDemo.exe.manifest'
$AppConfigName = 'InformationDemo.exe.config'
$DeploymentManifestName = 'InformationDemo.application'
$AppIdentityName = 'InformationDemo'
$AppDisplayName = "Informaci$([char]0x00F3)n del equipo"
$PublisherName = 'playwright-desktop'
$ProcessorArch = 'amd64'
$TargetFrameworkVersion = 'v4.0'
$TargetFrameworkSku = '.NETFramework,Version=v4.8'
$MagePackageId = 'Microsoft.DotNet.Mage'
$MageVersion = '10.0.0'
$MageExecutableName = 'dotnet-mage.exe'
$AllowedScheme = 'http'
$AllowedHosts = @('127.0.0.1', 'localhost')
$CSharpCompilerRelativePath = 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
$CSharpReferences = @(
    '/reference:System.Deployment.dll',
    '/reference:System.Web.dll',
    '/reference:System.Configuration.dll',
    '/reference:System.Windows.Forms.dll'
)

$demoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$javaRoot = Join-Path $demoRoot 'desktop-java'
$buildRoot = Join-Path $javaRoot 'build'
$publishRoot = Join-Path $PSScriptRoot 'publish'
$buildTime = [DateTime]::UtcNow
$buildVersion = "1.0.$($buildTime.DayOfYear).$([int][Math]::Floor($buildTime.TimeOfDay.TotalSeconds / 2))"
$payloadRelative = "Application Files/${AppIdentityName}_$($buildVersion.Replace('.', '_'))"
$payloadRoot = Join-Path $publishRoot $payloadRelative
$classRoot = Join-Path $buildRoot 'classes'
$toolRoot = Join-Path $PSScriptRoot '.tools'

if (-not $JdkHome -or -not (Test-Path -LiteralPath (Join-Path $JdkHome 'bin/javac.exe'))) {
    $compiler = Get-Command javac.exe -ErrorAction Stop
    $JdkHome = Split-Path (Split-Path $compiler.Source)
}
if (-not $JavaHome) {
    $JavaHome = $JdkHome
}
$JavaHome = [System.IO.Path]::GetFullPath($JavaHome)
if (-not (Test-Path -LiteralPath (Join-Path $JavaHome 'bin/javaw.exe'))) {
    throw 'JavaHome debe contener bin/javaw.exe.'
}
$publishUri = [Uri]$BaseUrl
if ($publishUri.Scheme -ne $AllowedScheme -or $publishUri.Host -notin $AllowedHosts -or $publishUri.AbsolutePath -ne '/') {
    throw 'BaseUrl debe ser la dirección HTTP local de la demo, por ejemplo http://127.0.0.1:18765.'
}

$applicationFilesRoot = Join-Path $publishRoot 'Application Files'
if (Test-Path -LiteralPath $applicationFilesRoot) {
    Remove-Item -LiteralPath $applicationFilesRoot -Recurse -Force
}
foreach ($directory in @($classRoot, $payloadRoot, $toolRoot)) {
    [void][System.IO.Directory]::CreateDirectory($directory)
}

& (Join-Path $JdkHome 'bin/javac.exe') --release $JavaReleaseVersion -encoding UTF-8 -d $classRoot (Join-Path $javaRoot $JavaSourceRelativePath)
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación de Java.' }
$javaJar = Join-Path $buildRoot $JarFileName
& (Join-Path $JdkHome 'bin/jar.exe') --create --file $javaJar --main-class $MainClassName -C $classRoot .
if ($LASTEXITCODE -ne 0) { throw 'Falló la creación del JAR.' }
Copy-Item -LiteralPath $javaJar -Destination $payloadRoot

$csharp = Join-Path $env:WINDIR $CSharpCompilerRelativePath
# El manifiesto Win32 predeterminado de csc identifica MyApplication.app y
# entra en conflicto con la identidad administrada al validar ClickOnce.
& $csharp /nologo /target:winexe /platform:x64 /nowin32manifest "/out:$(Join-Path $payloadRoot $LauncherExeName)" @CSharpReferences (Join-Path $PSScriptRoot $LauncherSourceFile)
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación del lanzador C#.' }

$escapedJavaHome = [System.Security.SecurityElement]::Escape($JavaHome)
$configuration = @"
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <startup><supportedRuntime version="$TargetFrameworkVersion" sku="$TargetFrameworkSku" /></startup>
  <appSettings>
    <add key="JavaHome" value="$escapedJavaHome" />
  </appSettings>
</configuration>
"@
[System.IO.File]::WriteAllText((Join-Path $payloadRoot $AppConfigName), $configuration, [System.Text.UTF8Encoding]::new($false))

$mage = Join-Path $toolRoot $MageExecutableName
if (-not (Test-Path -LiteralPath $mage)) {
    dotnet tool install $MagePackageId --version $MageVersion --tool-path $toolRoot --allow-roll-forward
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo instalar Microsoft.DotNet.Mage.' }
}
$applicationManifest = Join-Path $payloadRoot $AppManifestName
# Evitar incluir el manifiesto anterior dentro de sí mismo al recompilar.
if (Test-Path -LiteralPath $applicationManifest) { Remove-Item -LiteralPath $applicationManifest }
& $mage -New Application -ToFile $applicationManifest -Name $AppIdentityName -Version $buildVersion -Processor $ProcessorArch -FromDirectory $payloadRoot
if ($LASTEXITCODE -ne 0) { throw 'No se pudo generar el manifiesto de aplicación.' }
$deploymentManifest = Join-Path $publishRoot $DeploymentManifestName
$providerUrl = $BaseUrl.TrimEnd('/') + "/clickonce/$DeploymentManifestName"
& $mage -New Deployment -ToFile $deploymentManifest -Name $AppDisplayName -Version $buildVersion -Processor $ProcessorArch -AppManifest $applicationManifest -AppCodeBase "$payloadRelative/$AppManifestName" -ProviderUrl $providerUrl -Install false -TrustURLParameters true -Publisher $PublisherName
if ($LASTEXITCODE -ne 0) { throw 'No se pudo generar la publicación ClickOnce.' }

Write-Output "Publicación lista en $publishRoot"
Write-Output "Runtime Java configurado en $JavaHome"
