[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",
    [string]$PinRoot = $env:PIN_ROOT
)

$ErrorActionPreference = "Stop"
if (-not $PinRoot) { throw "Pass -PinRoot or set PIN_ROOT to the Intel Pin 3.31 SDK root." }
$PinRoot = (Resolve-Path -LiteralPath $PinRoot).Path
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.Component.MSBuild -property installationPath
if (-not $vs) { throw "Visual Studio MSBuild was not found." }
$msbuild = Join-Path $vs "MSBuild\Current\Bin\MSBuild.exe"
$project = Join-Path $PSScriptRoot "msvc\pinbridge_pin.vcxproj"

# The managed sandbox exposes both Path and PATH. MSBuild treats them as duplicate
# dictionary keys when spawning cl.exe, so remove one spelling in the child shell.
$command = 'set Path=&& "' + $msbuild + '" "' + $project + '" /m /t:Build ' +
    '/p:Configuration=' + $Configuration + ' /p:Platform=x64 /p:PinRoot="' + $PinRoot + '"'
& cmd.exe /d /s /c $command
if ($LASTEXITCODE -ne 0) { throw "PinBridge PinTool build failed." }

$dll = Join-Path $PSScriptRoot "build\pin\x64\$Configuration\pinbridge.dll"
if (-not (Test-Path -LiteralPath $dll)) { throw "Build completed without pinbridge.dll." }
Get-Item -LiteralPath $dll | Select-Object FullName, Length, LastWriteTime
