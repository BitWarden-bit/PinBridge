# diag_launch.ps1: clean-launch the rpc fixture under the agent, stderr
# captured. Usage: powershell -File diag_launch.ps1 <tag>
param([string]$Tag = "x")

$repo = Split-Path -Parent $PSScriptRoot
$rel = Join-Path $repo "bindings\rust\target\release"
$pin = $env:PINBRIDGE_PIN_EXE
if (-not $pin -and $env:PIN_ROOT) { $pin = Join-Path $env:PIN_ROOT "intel64\bin\pin.exe" }
if (-not $pin) { throw "pin.exe not found: set PINBRIDGE_PIN_EXE to the full path of pin.exe (or PIN_ROOT to your Pin 3.31 SDK root)" }
$agent = Join-Path $rel "pinbridge_agent.dll"
$fixture = Join-Path $repo "build\host-tests\pb_rpc_fixture.exe"
$info = Join-Path $rel "rpc_fixture_info.txt"
$log = Join-Path $rel "diag_$Tag.log"
$err = Join-Path $rel "diag_$Tag.stderr.txt"

Get-CimInstance Win32_Process |
    Where-Object { $_.Name -in @('pb_rpc_fixture.exe','pin.exe') } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
Start-Sleep -Milliseconds 800
Remove-Item $info, $log, $err -ErrorAction SilentlyContinue

$env:PINBRIDGE_AGENT_PORT = "9011"
$env:PINBRIDGE_AGENT_LOG = $log
$proc = Start-Process -FilePath $pin `
    -ArgumentList '-t', ('"' + $agent + '"'), '--', ('"' + $fixture + '"'), '--pinbridge-rpc-info', ('"' + $info + '"') `
    -WorkingDirectory $rel -RedirectStandardError $err -PassThru
Write-Output "launched pin pid=$($proc.Id)"
