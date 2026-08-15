[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$source = Join-Path $dir "lifecycle_demo.c"
$output = Join-Path $dir "lifecycle_demo_x64.exe"
$object = Join-Path $dir "lifecycle_demo_x64.obj"
$moduleSource = Join-Path $dir "lifecycle_module.c"
$moduleOutput = Join-Path $dir "lifecycle_module_x64.dll"
$moduleObject = Join-Path $dir "lifecycle_module_x64.obj"
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path -LiteralPath $vswhere)) { throw "vswhere.exe not found" }
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
if (-not $vs) { throw "MSVC x64 workload not found" }
$devcmd = Join-Path $vs "Common7\Tools\VsDevCmd.bat"
$line = '"{0}" -no_logo -arch=x64 && cl.exe /nologo /O2 /W3 /LD /Fo:"{5}" /Fe:"{4}" "{3}" && cl.exe /nologo /O2 /W3 /Fo:"{2}" /Fe:"{1}" "{6}"' -f $devcmd, $output, $object, $moduleSource, $moduleOutput, $moduleObject, $source
& cmd.exe /d /s /c $line
if ($LASTEXITCODE -ne 0 -or
    -not (Test-Path -LiteralPath $output) -or
    -not (Test-Path -LiteralPath $moduleOutput)) {
    throw "failed to build lifecycle fixture"
}
Write-Output "built $output and $moduleOutput"
