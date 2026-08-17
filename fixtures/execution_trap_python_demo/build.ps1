$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$source = Join-Path $dir "execution_trap_demo.c"
$object = Join-Path $dir "execution_trap_demo_x64.obj"
$target = Join-Path $dir "execution_trap_demo_x64.exe"

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$devcmd = Join-Path $vs "Common7\Tools\VsDevCmd.bat"
$command = 'call "{0}" -arch=x64 -host_arch=x64 >nul && cl /nologo /W4 /O2 /Fo"{1}" /Fe"{3}" "{2}"' -f $devcmd, $object, $source, $target
cmd.exe /d /c $command
if ($LASTEXITCODE -ne 0) { throw "fixture build failed: $LASTEXITCODE" }
Get-Item -LiteralPath $target | Select-Object FullName,Length,LastWriteTime
