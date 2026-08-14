[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release"
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$build = Join-Path $root "build\host-tests"
$cmake = "C:\Program Files\CMake\bin\cmake.exe"
if (-not (Test-Path -LiteralPath $cmake)) {
    $cmake = (Get-Command cmake.exe -ErrorAction Stop).Source
}
$ctest = Join-Path (Split-Path -Parent $cmake) "ctest.exe"
$python = (Get-Command python.exe -ErrorAction Stop).Source
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw "Visual Studio C++ tools were not found." }
$toolset = Get-ChildItem -LiteralPath (Join-Path $vs "VC\Tools\MSVC") -Directory |
    Sort-Object Name -Descending | Select-Object -First 1
$nmake = Join-Path $toolset.FullName "bin\Hostx64\x64\nmake.exe"
$devcmd = Join-Path $vs "Common7\Tools\VsDevCmd.bat"

function Invoke-DeveloperCommand([string]$Command) {
    $line = '"' + $devcmd + '" -no_logo -arch=x64 -host_arch=x64 && ' + $Command
    & cmd.exe /d /s /c $line
    if ($LASTEXITCODE -ne 0) { throw "Developer command failed with exit code $LASTEXITCODE." }
}

$configure = '"' + $cmake + '" --fresh -S "' + $root + '" -B "' + $build +
    '" -G "NMake Makefiles" -DCMAKE_BUILD_TYPE=' + $Configuration +
    ' -DCMAKE_MAKE_PROGRAM="' + $nmake + '"'
Invoke-DeveloperCommand $configure
Invoke-DeveloperCommand ('"' + $cmake + '" --build "' + $build + '"')

& $ctest --test-dir $build --output-on-failure
if ($LASTEXITCODE -ne 0) { throw "Contract tests failed." }

& $python (Join-Path $root "tools\generate_rust_bindings.py")
if ($LASTEXITCODE -ne 0) { throw "Rust binding generation failed." }

$dll = Join-Path $root "build\pin\x64\$Configuration\pinbridge.dll"
if (Test-Path -LiteralPath $dll) {
    & powershell.exe -NoProfile -ExecutionPolicy Bypass `
        -File (Join-Path $root "tests\Check-Exports.ps1") -Configuration $Configuration
    if ($LASTEXITCODE -ne 0) { throw "Export check failed." }
} else {
    "Skipping Check-Exports.ps1: $dll not found. Run Build-Pin.ps1 first to build pinbridge.dll."
}

"Run-Tests.ps1 completed."
