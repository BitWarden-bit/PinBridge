[CmdletBinding()]
param(
    [ValidateSet("x86", "x64")]
    [string]$Arch = "x64"
)

$ErrorActionPreference = "Stop"
$dir = Split-Path -Parent $MyInvocation.MyCommand.Path
$source = Join-Path $dir "hook_action_demo.c"
$output = Join-Path $dir ("hook_action_demo_{0}.exe" -f $Arch)
$object = Join-Path $dir ("hook_action_demo_{0}.obj" -f $Arch)
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path -LiteralPath $vswhere)) { throw "vswhere.exe not found" }
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
if (-not $vs) { throw "MSVC x86/x64 workload not found" }
$devcmd = Join-Path $vs "Common7\Tools\VsDevCmd.bat"
$platform = if ($Arch -eq "x86") { "x86" } else { "x64" }
$line = '"{0}" -no_logo -arch={1} && cl.exe /nologo /O2 /W3 /EHsc /Fo:"{4}" /Fe:"{2}" "{3}"' -f $devcmd, $platform, $output, $source, $object
& cmd.exe /d /s /c $line
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $output)) {
    throw "failed to build $Arch fixture"
}
Write-Output "built $output"
