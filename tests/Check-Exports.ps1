[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",
    [ValidateSet("x64", "ia32")]
    [string]$Arch = "x64"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dll = Join-Path $root "build\pin\$Arch\$Configuration\pinbridge.dll"
$expected = Join-Path $PSScriptRoot "expected_exports.txt"
if (-not (Test-Path -LiteralPath $dll)) { throw "Missing $dll. Run Build-Pin.ps1 first." }
if (-not (Test-Path -LiteralPath $expected)) { throw "Missing $expected. Run tools/generate_rust_bindings.py first." }

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products * -requires Microsoft.Component.MSBuild -property installationPath
if (-not $vs) { throw "Visual Studio not found." }
$dumpbin = Get-ChildItem -Path (Join-Path $vs "VC\Tools\MSVC") -Recurse -Filter dumpbin.exe |
    Where-Object { $_.FullName -like "*Hostx64\x64*" } | Select-Object -First 1 -ExpandProperty FullName
if (-not $dumpbin) { throw "dumpbin.exe not found under $vs." }

$actual = & $dumpbin /exports $dll |
    ForEach-Object { ($_ -split '\s+') | Where-Object { $_ -like 'pb_*' } } |
    Sort-Object -Unique
$wanted = Get-Content -LiteralPath $expected | Where-Object { $_ }

# pb_toolhost_* are deliberate non-ABI exports (tool-host glue for foreign
# language tool DLLs, see src/pin_client_glue.cpp); they are not declared in
# pinbridge.h and stay exempt from the header parse comparison.
$glue = @("pb_toolhost_client_int", "pb_toolhost_commit_hash")
$actual = $actual | Where-Object { $glue -notcontains $_ }

$missing = $wanted | Where-Object { $actual -notcontains $_ }
$extra = $actual | Where-Object { $wanted -notcontains $_ }

"{0} expected pb_* symbols, {1} pb_* exports in {2}" -f $wanted.Count, $actual.Count, (Split-Path -Leaf $dll)
if ($missing) { $missing | ForEach-Object { "ERROR: missing export: $_" } }
if ($extra) { $extra | ForEach-Object { "ERROR: export not in header parse: $_" } }
if ($missing -or $extra) { exit 1 }
"All pb_* exports match the header parse."
