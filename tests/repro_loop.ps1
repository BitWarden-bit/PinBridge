# repro_loop.ps1: launch fixture, load the probe script, watch survival. Repeat.
param([int]$Rounds = 10)

$repo = Split-Path -Parent $PSScriptRoot
$rel = Join-Path $repo "bindings\rust\target\release"
$pin = $env:PINBRIDGE_PIN_EXE
if (-not $pin -and $env:PIN_ROOT) { $pin = Join-Path $env:PIN_ROOT "intel64\bin\pin.exe" }
if (-not $pin) { throw "pin.exe not found: set PINBRIDGE_PIN_EXE to the full path of pin.exe (or PIN_ROOT to your Pin 3.31 SDK root)" }
$agent = Join-Path $rel "pinbridge_agent.dll"
$fixture = Join-Path $repo "build\host-tests\pb_rpc_fixture.exe"
$cli = Join-Path $rel "pinbridge-cli.exe"
$info = Join-Path $rel "rpc_fixture_info.txt"
$probe = Join-Path $rel "script_e2e_probe.py"

for ($round = 1; $round -le $Rounds; $round++) {
    Get-CimInstance Win32_Process |
        Where-Object { $_.Name -in @('pb_rpc_fixture.exe','pin.exe') } |
        ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Milliseconds 500
    $log = Join-Path $rel ("repro_{0}.log" -f $round)
    Remove-Item $info, $log -ErrorAction SilentlyContinue

    $env:PINBRIDGE_AGENT_PORT = "9011"
    $env:PINBRIDGE_AGENT_LOG = $log
    $pinProc = Start-Process -FilePath $pin `
        -ArgumentList '-t', ('"' + $agent + '"'), '--', ('"' + $fixture + '"'), '--pinbridge-rpc-info', ('"' + $info + '"') `
        -WorkingDirectory $rel -PassThru

    $ready = $false
    for ($i = 0; $i -lt 25; $i++) {
        if (Test-Path $info) { $ready = $true; break }
        Start-Sleep -Milliseconds 400
    }
    if (-not $ready) { Write-Output "round ${round}: NO INFO (start slow)"; continue }

    & $cli --port 9011 script run $probe | Out-Null
    Start-Sleep -Seconds 3
    $alive = [bool](Get-Process pb_rpc_fixture -ErrorAction SilentlyContinue)
    if ($alive) {
        Write-Output "round ${round}: ALIVE"
    } else {
        Write-Output "round ${round}: DEAD"
        Get-Content $log -Tail 6 | ForEach-Object { Write-Output ("  " + $_) }
    }
}
Get-CimInstance Win32_Process |
    Where-Object { $_.Name -in @('pb_rpc_fixture.exe','pin.exe') } |
    ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
