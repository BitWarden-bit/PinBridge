[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$source = Join-Path $dir "memory_translation_python_demo.c"
$output = Join-Path $dir "memory_translation_python_demo_x64.exe"
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path -LiteralPath $vswhere)) { throw "vswhere.exe not found" }
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "Visual C++ toolchain not found" }
$devcmd = Join-Path $vs "Common7\Tools\VsDevCmd.bat"
if (-not (Test-Path -LiteralPath $devcmd)) { throw "VsDevCmd.bat not found" }
$command = 'call "{0}" -arch=x64 -host_arch=x64 >nul && cl /nologo /O2 /W4 /D_CRT_SECURE_NO_WARNINGS "{1}" /link /out:"{2}"' -f $devcmd, $source, $output
& cmd.exe /d /s /c $command
if ($LASTEXITCODE -ne 0) { throw "cl.exe failed with exit code $LASTEXITCODE" }
Write-Host "built $output"
