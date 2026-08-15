[CmdletBinding()]
param()

# Builds hello32.exe as a real PE32 (I386) image, or exits SKIP when this
# machine has no 32-bit C compiler. The exe is never fabricated: it is only
# ever produced by a real compiler invocation and is then verified against the
# PE32 header expectations before it is kept.
#
# Exit codes (documented in README.md):
#   0  - hello32.exe built and verified as PE32/I386
#   77 - SKIP: no usable 32-bit C compiler was found (no exe produced)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Source = Join-Path $ScriptDir "hello32.c"
$Output = Join-Path $ScriptDir "hello32.exe"
$TmpOut = Join-Path $ScriptDir "hello32.tmp.exe"

# The autotools "skip" convention, repurposed here so CI can distinguish
# "not built" from "built" without scanning output text.
$SKIP_EXIT = 77

# --- PE32 verification ----------------------------------------------------
# Reads only the DOS + COFF + optional headers and checks that the image is
# actually I386 + PE32. This is the gate that guarantees no forged PE is kept.

function Test-Pe32([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path)) { return $false }
    $bytes = [System.IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 0x40) { return $false }
    # "MZ"
    if ($bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) { return $false }
    $peOffset = [System.BitConverter]::ToUInt32($bytes, 0x3C)
    if ($peOffset + 24 -gt $bytes.Length) { return $false }
    # "PE\0\0" (the two zero bytes need not be checked for arch selection)
    if ($bytes[$peOffset] -ne 0x50 -or $bytes[$peOffset + 1] -ne 0x45) { return $false }
    # COFF Machine field sits right after the 4-byte signature.
    $machine = [System.BitConverter]::ToUInt16($bytes, $peOffset + 4)
    # Optional-header Magic sits 20 bytes into the COFF file header.
    $magic = [System.BitConverter]::ToUInt16($bytes, $peOffset + 24)
    return ($machine -eq 0x014C -and $magic -eq 0x010B)  # I386 + PE32
}

# --- compiler discovery ----------------------------------------------------

function Get-MsvcX86Cl {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswhere)) { return $null }
    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if (-not $vs) { return $null }
    $msvcRoot = Join-Path $vs "VC\Tools\MSVC"
    if (-not (Test-Path -LiteralPath $msvcRoot)) { return $null }
    $toolset = Get-ChildItem -LiteralPath $msvcRoot -Directory -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending | Select-Object -First 1
    if (-not $toolset) { return $null }
    # x86 target compilers, host-x64 first then host-x86.
    foreach ($rel in @("bin\Hostx64\x86\cl.exe", "bin\Hostx86\x86\cl.exe")) {
        $cl = Join-Path $toolset.FullName $rel
        if (Test-Path -LiteralPath $cl) { return $cl }
    }
    return $null
}

function Get-MsvcDevCmd {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (-not (Test-Path -LiteralPath $vswhere)) { return $null }
    $vs = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if (-not $vs) { return $null }
    $devcmd = Join-Path $vs "Common7\Tools\VsDevCmd.bat"
    if (Test-Path -LiteralPath $devcmd) { return $devcmd }
    return $null
}

# --- build attempts --------------------------------------------------------
# Each attempt compiles into $TmpOut and returns $true only when the compiler
# exited cleanly AND $TmpOut verifies as PE32/I386. A failed attempt never
# leaves a partial or bogus exe behind.

function Try-Msvc([string]$Cl, [string]$DevCmd) {
    Push-Location $ScriptDir
    try {
        Remove-Item -LiteralPath $TmpOut -ErrorAction SilentlyContinue
        $line = '"' + $DevCmd + '" -no_logo -arch=x86 && "' + $Cl + '" /nologo /O2 /Fe:' + $TmpOut + ' hello32.c'
        & cmd.exe /d /s /c $line
        $code = $LASTEXITCODE
        if ($code -ne 0 -or -not (Test-Pe32 $TmpOut)) {
            Remove-Item -LiteralPath $TmpOut -ErrorAction SilentlyContinue
            return $false
        }
        return $true
    } finally {
        Pop-Location
    }
}

function Try-Gcc([string]$Gcc) {
    Push-Location $ScriptDir
    try {
        Remove-Item -LiteralPath $TmpOut -ErrorAction SilentlyContinue
        & $Gcc -m32 -O2 -o $TmpOut hello32.c 2>$null
        $code = $LASTEXITCODE
        if ($code -ne 0 -or -not (Test-Pe32 $TmpOut)) {
            Remove-Item -LiteralPath $TmpOut -ErrorAction SilentlyContinue
            return $false
        }
        return $true
    } finally {
        Pop-Location
    }
}

function Try-ClangCl([string]$ClangCl) {
    Push-Location $ScriptDir
    try {
        Remove-Item -LiteralPath $TmpOut -ErrorAction SilentlyContinue
        & $ClangCl --target=i386-pc-windows-msvc /nologo /O2 /Fe:$TmpOut hello32.c 2>$null
        $code = $LASTEXITCODE
        if ($code -ne 0 -or -not (Test-Pe32 $TmpOut)) {
            Remove-Item -LiteralPath $TmpOut -ErrorAction SilentlyContinue
            return $false
        }
        return $true
    } finally {
        Pop-Location
    }
}

# --- main ------------------------------------------------------------------

if (-not (Test-Path -LiteralPath $Source)) {
    Write-Error "missing fixture source: $Source"
    exit 1
}

$compiler = $null
$built = $false

# 1) MSVC x86 cl (the primary path).
$cl = Get-MsvcX86Cl
if ($cl) {
    $devcmd = Get-MsvcDevCmd
    if ($devcmd -and (Try-Msvc $cl $devcmd)) {
        $compiler = "MSVC x86 cl ($cl)"
        $built = $true
    }
}

# 2) clang-cl targeting i386 (works when MSVC libs/headers are present).
if (-not $built) {
    $clangCl = Get-Command clang-cl.exe -ErrorAction SilentlyContinue
    if ($clangCl -and (Try-ClangCl $clangCl.Source)) {
        $compiler = "clang-cl ($($clangCl.Source))"
        $built = $true
    }
}

# 3) gcc -m32 (MinGW-w64 i686).
if (-not $built) {
    $gcc = Get-Command gcc.exe -ErrorAction SilentlyContinue
    if ($gcc -and (Try-Gcc $gcc.Source)) {
        $compiler = "gcc -m32 ($($gcc.Source))"
        $built = $true
    }
}

if (-not $built) {
    # Explicit SKIP: no usable 32-bit compiler. Do not leave any exe behind,
    # and never fabricate a PE.
    Remove-Item -LiteralPath $TmpOut -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath (Join-Path $ScriptDir "hello32.obj") -ErrorAction SilentlyContinue
    Write-Output "SKIP: no 32-bit C compiler found (tried MSVC x86 cl, clang-cl, gcc -m32); install the Visual Studio 'MSVC v143 x86/x64 build tools' workload and re-run."
    exit $SKIP_EXIT
}

# Success: move the verified PE32 into place and drop transient artifacts.
Remove-Item -LiteralPath (Join-Path $ScriptDir "hello32.obj") -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $Output) { Remove-Item -LiteralPath $Output -Force }
Move-Item -LiteralPath $TmpOut -Destination $Output

Write-Output "built $Output ($compiler)"
Write-Output "  machine=0x014C (I386) optional_magic=0x010B (PE32)"
exit 0
