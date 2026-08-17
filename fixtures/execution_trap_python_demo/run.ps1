[CmdletBinding()]
param([ValidateRange(15, 90)][int]$TimeoutSec = 45)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$target = Join-Path $dir "execution_trap_demo_x64.exe"
$plugin = Join-Path $dir "execution_trap.py"
$cli = Join-Path $repo "bindings\rust\target\release\pinbridge-cli.exe"
$agent = Join-Path $repo "bindings\rust\target\release\pinbridge_agent.dll"
$pin = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\intel64\bin\pin.exe"
$ready = Join-Path $dir "execution_trap.ready"

foreach ($path in @($target, $plugin, $cli, $agent, $pin)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "required file not found: $path" }
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    try { $listener.Start(); return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port }
    finally { $listener.Stop() }
}

function Invoke-Cli([string[]]$Command) {
    $output = & $script:Cli --port $script:Port @Command 2>&1
    if ($LASTEXITCODE -ne 0) { throw "pinbridge-cli failed: $output" }
    return ($output -join "`n")
}

$port = Get-FreePort
$script:Port = $port
$script:Cli = $cli
$log = Join-Path $dir ("execution_trap_{0}.agent.log" -f $port)
$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldLog = $env:PINBRIDGE_AGENT_LOG
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$process = $null

try {
    Remove-Item -LiteralPath $ready -Force -ErrorAction SilentlyContinue
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_LOG = $log
    $env:PINBRIDGE_AGENT_ENGINES = "none"
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $pin
    $start.Arguments = '-t "{0}" -- "{1}"' -f $agent, $target
    $start.WorkingDirectory = $dir
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    [void]$process.Start()
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()

    $deadline = [datetime]::UtcNow.AddSeconds($TimeoutSec)
    while ([datetime]::UtcNow -lt $deadline) {
        try { [void](Invoke-Cli @("ping")); break } catch { Start-Sleep -Milliseconds 50 }
    }
    [void](Invoke-Cli @("script", "run", $plugin))

    $captured = ""
    while ([datetime]::UtcNow -lt $deadline -and -not $process.HasExited) {
        try { $captured = Invoke-Cli @("script", "output") } catch {}
        Start-Sleep -Milliseconds 50
    }
    if (-not $process.HasExited) { throw "target did not exit" }
    $process.WaitForExit()
    if (Test-Path -LiteralPath $log) { $captured += "`n" + (Get-Content -LiteralPath $log -Raw) }
    if ($process.ExitCode -ne 0 -or $stdout.Result.Trim() -ne "trap_target=42") {
        throw "target failed exit=$($process.ExitCode) stdout=$($stdout.Result) stderr=$($stderr.Result)"
    }
    foreach ($marker in @("EXECUTION_TRAP_READY", "EXECUTION_TRAP_HIT")) {
        if (-not $captured.Contains($marker)) { throw "missing marker: $marker" }
    }
    [ordered]@{
        result = "EXECUTION_TRAP_PYTHON_PASS"
        target_exit = $process.ExitCode
        target_output = $stdout.Result.Trim()
        exact_pre_instruction_stop = $true
    } | ConvertTo-Json
} finally {
    if ($null -ne $process -and -not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
    Remove-Item -LiteralPath $ready -Force -ErrorAction SilentlyContinue
    if ($null -eq $oldPort) { Remove-Item Env:PINBRIDGE_AGENT_PORT -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_PORT = $oldPort }
    if ($null -eq $oldLog) { Remove-Item Env:PINBRIDGE_AGENT_LOG -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_LOG = $oldLog }
    if ($null -eq $oldEngines) { Remove-Item Env:PINBRIDGE_AGENT_ENGINES -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_ENGINES = $oldEngines }
}
