[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$source = Join-Path $dir "exception_python_demo.c"
$output = Join-Path $dir "exception_python_demo_x64.exe"
$object = Join-Path $dir "exception_python_demo_x64.obj"
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path -LiteralPath $vswhere)) { throw "vswhere.exe not found" }
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
if (-not $vs) { throw "MSVC x64 workload not found" }
$devcmd = Join-Path $vs "Common7\Tools\VsDevCmd.bat"
$line = '"{0}" -no_logo -arch=x64 && cl.exe /nologo /O2 /W3 /Fo:"{3}" /Fe:"{1}" "{2}"' -f $devcmd, $output, $source, $object
& cmd.exe /d /s /c $line
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $output)) {
    throw "failed to build synchronous exception fixture"
}
Write-Output "built $output"
