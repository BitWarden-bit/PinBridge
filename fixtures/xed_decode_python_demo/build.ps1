[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
if (-not $vs) { throw "MSVC x64 workload not found" }
$devcmd = Join-Path $vs "Common7\Tools\VsDevCmd.bat"
$cSource = Join-Path $dir "xed_decode_python_demo.c"
$asmSource = Join-Path $dir "xed_decode_python_demo.asm"
$cObject = Join-Path $dir "xed_decode_python_demo.obj"
$asmObject = Join-Path $dir "xed_decode_target.obj"
$output = Join-Path $dir "xed_decode_python_demo_x64.exe"
$line = '"{0}" -no_logo -arch=x64 && cl.exe /nologo /O2 /W3 /c /Fo:"{1}" "{2}" && ml64.exe /nologo /c /Fo"{3}" "{4}" && link.exe /nologo /OUT:"{5}" /EXPORT:DecodeTarget "{1}" "{3}" kernel32.lib' -f $devcmd, $cObject, $cSource, $asmObject, $asmSource, $output
& cmd.exe /d /s /c $line
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $output)) {
    throw "failed to build XED decode fixture"
}
Write-Output "built $output"
