[CmdletBinding()]
param(
    [bool]$BuildX64 = $true,
    [bool]$BuildX86 = $true,
    [string]$X86Toolchain = "1.94.1-x86_64-pc-windows-msvc",
    [string]$X86PythonDist = ""
)

$ErrorActionPreference = "Stop"
$rustRoot = $PSScriptRoot
$x86Target = "i686-pc-windows-msvc"
$pythonVersion = "3.10.11"
$pythonArchiveName = "python-$pythonVersion-embed-win32.zip"
$pythonArchiveUrl = "https://www.python.org/ftp/python/$pythonVersion/$pythonArchiveName"
$pythonArchiveSha256 = "0987A9D85CCF1BA17C3DBDCADC39835F183843604DA18C9AF4BD677DC84ADF7D"

function Test-PeMachine([string]$Path, [uint16]$Expected) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $false }
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -lt 0x40 -or $bytes[0] -ne 0x4D -or $bytes[1] -ne 0x5A) {
        return $false
    }
    $offset = [BitConverter]::ToUInt32($bytes, 0x3C)
    if ($offset + 6 -gt $bytes.Length) { return $false }
    if ($bytes[$offset] -ne 0x50 -or $bytes[$offset + 1] -ne 0x45 -or
        $bytes[$offset + 2] -ne 0 -or $bytes[$offset + 3] -ne 0) {
        return $false
    }
    return [BitConverter]::ToUInt16($bytes, $offset + 4) -eq $Expected
}

function Get-X86PythonDist {
    if ($X86PythonDist) {
        $resolved = (Resolve-Path -LiteralPath $X86PythonDist).Path
        if (-not (Test-PeMachine (Join-Path $resolved "python310.dll") 0x014C)) {
            throw "X86PythonDist does not contain an x86 python310.dll: $resolved"
        }
        if (-not (Test-Path -LiteralPath (Join-Path $resolved "python310.zip") -PathType Leaf)) {
            throw "X86PythonDist does not contain python310.zip: $resolved"
        }
        return $resolved
    }

    $cacheRoot = Join-Path $env:LOCALAPPDATA "PinBridge\python-$pythonVersion-embed-win32"
    $archive = Join-Path $cacheRoot $pythonArchiveName
    $dist = Join-Path $cacheRoot "dist"
    New-Item -ItemType Directory -Path $cacheRoot -Force | Out-Null
    if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
        Write-Host "downloading official CPython $pythonVersion x86 embeddable package"
        Invoke-WebRequest -Uri $pythonArchiveUrl -OutFile $archive
    }
    $actualHash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    if ($actualHash -ne $pythonArchiveSha256) {
        throw "CPython x86 archive SHA-256 mismatch: $actualHash"
    }
    if (-not (Test-PeMachine (Join-Path $dist "python310.dll") 0x014C)) {
        New-Item -ItemType Directory -Path $dist -Force | Out-Null
        Expand-Archive -LiteralPath $archive -DestinationPath $dist -Force
    }
    if (-not (Test-PeMachine (Join-Path $dist "python310.dll") 0x014C) -or
        -not (Test-Path -LiteralPath (Join-Path $dist "python310.zip") -PathType Leaf)) {
        throw "provisioned CPython distribution is incomplete or not x86: $dist"
    }
    return $dist
}

$savedEnvironment = @{}
foreach ($name in @(
    "RUSTFLAGS", "RUSTUP_DIST_SERVER", "RUSTUP_UPDATE_ROOT", "PYO3_CROSS",
    "PYO3_CROSS_PYTHON_VERSION", "PYO3_CROSS_PYTHON_IMPLEMENTATION",
    "PINBRIDGE_PYTHON_DIST"
)) {
    $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}

Push-Location $rustRoot
try {
    Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue
    Remove-Item Env:PYO3_CROSS -ErrorAction SilentlyContinue
    Remove-Item Env:PYO3_CROSS_PYTHON_VERSION -ErrorAction SilentlyContinue
    Remove-Item Env:PYO3_CROSS_PYTHON_IMPLEMENTATION -ErrorAction SilentlyContinue
    Remove-Item Env:PINBRIDGE_PYTHON_DIST -ErrorAction SilentlyContinue

    if ($BuildX64) {
        Write-Host "building x64 Python agent and controller"
        & cargo build --release -p pinbridge-agent -p pinbridge-cli
        if ($LASTEXITCODE -ne 0) { throw "x64 Cargo build failed" }
        $x64Agent = Join-Path $rustRoot "target\release\pinbridge_agent.dll"
        if (-not (Test-PeMachine $x64Agent 0x8664)) {
            throw "x64 agent PE verification failed: $x64Agent"
        }
    }

    if ($BuildX86) {
        $installedToolchains = @(& rustup toolchain list)
        if (-not ($installedToolchains | Where-Object { $_ -like "$X86Toolchain*" })) {
            Write-Host "installing isolated Rust toolchain $X86Toolchain"
            $env:RUSTUP_DIST_SERVER = "https://static.rust-lang.org"
            $env:RUSTUP_UPDATE_ROOT = "https://static.rust-lang.org/rustup"
            & rustup toolchain install $X86Toolchain --profile minimal --target $x86Target
            if ($LASTEXITCODE -ne 0) { throw "Rust x86 toolchain installation failed" }
        }
        $installedTargets = @(& rustup target list --installed --toolchain $X86Toolchain)
        if ($installedTargets -notcontains $x86Target) {
            $env:RUSTUP_DIST_SERVER = "https://static.rust-lang.org"
            $env:RUSTUP_UPDATE_ROOT = "https://static.rust-lang.org/rustup"
            & rustup target add --toolchain $X86Toolchain $x86Target
            if ($LASTEXITCODE -ne 0) { throw "Rust i686 standard library installation failed" }
        }

        $dist = Get-X86PythonDist
        $env:PYO3_CROSS = "1"
        $env:PYO3_CROSS_PYTHON_VERSION = "3.10"
        $env:PYO3_CROSS_PYTHON_IMPLEMENTATION = "CPython"
        $env:PINBRIDGE_PYTHON_DIST = $dist
        Write-Host "building x86 Python agent with $dist"
        & cargo ("+" + $X86Toolchain) build --target $x86Target --release -p pinbridge-agent
        if ($LASTEXITCODE -ne 0) { throw "x86 Python Cargo build failed" }
        $x86Dir = Join-Path $rustRoot "target\$x86Target\release"
        foreach ($file in @("pinbridge_agent.dll", "pinbridge.dll", "python310.dll", "python310.zip")) {
            if (-not (Test-Path -LiteralPath (Join-Path $x86Dir $file) -PathType Leaf)) {
                throw "x86 deployment file missing: $file"
            }
        }
        if (-not (Test-PeMachine (Join-Path $x86Dir "pinbridge_agent.dll") 0x014C) -or
            -not (Test-PeMachine (Join-Path $x86Dir "python310.dll") 0x014C)) {
            throw "x86 agent/Python PE verification failed"
        }
    }

    [ordered]@{
        x64 = $BuildX64
        x86 = $BuildX86
        x86_python = $BuildX86
        output_root = (Join-Path $rustRoot "target")
    } | ConvertTo-Json
} finally {
    Pop-Location
    foreach ($name in $savedEnvironment.Keys) {
        $value = $savedEnvironment[$name]
        if ($null -eq $value) {
            Remove-Item ("Env:" + $name) -ErrorAction SilentlyContinue
        } else {
            Set-Item ("Env:" + $name) $value
        }
    }
}
