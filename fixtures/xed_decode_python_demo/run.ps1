[CmdletBinding()]
param(
    [ValidateRange(30, 120)]
    [int]$TimeoutSec = 60
)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$target = Join-Path $dir "xed_decode_python_demo_x64.exe"
$plugin = Join-Path $dir "xed_policy.py"
$cli = Join-Path $repo "bindings\rust\target\release\pinbridge-cli.exe"
$agent = Join-Path $repo "bindings\rust\target\release\pinbridge_agent.dll"
$pin = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\intel64\bin\pin.exe"
$ready = Join-Path $dir "xed_decode.ready"
$go = Join-Path $dir "xed_decode.go"

foreach ($path in @($target, $plugin, $cli, $agent, $pin)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "required file not found: $path" }
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    try { $listener.Start(); return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port }
    finally { $listener.Stop() }
}

function Invoke-Cli([string[]]$Command) {
    $arguments = @("--port", $script:Port.ToString()) + $Command
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $script:Cli
    $start.Arguments = (@($arguments | ForEach-Object {
        '"' + ([string]$_).Replace('"', '\"') + '"'
    }) -join " ")
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $start
    [void]$process.Start()
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(15000)) { $process.Kill(); $process.WaitForExit(); throw "pinbridge-cli timed out" }
    if ($process.ExitCode -ne 0) { throw "pinbridge-cli failed: $($stderr.Result.Trim()) $($stdout.Result.Trim())" }
    return $stdout.Result.Trim()
}

$port = Get-FreePort
$script:Port = $port
$script:Cli = $cli
$log = Join-Path $dir ("xed_decode_python_{0}.agent.log" -f $port)
$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldLog = $env:PINBRIDGE_AGENT_LOG
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$pinProcess = $null

try {
    foreach ($path in @($ready, $go)) {
        if (Test-Path -LiteralPath $path -PathType Leaf) { Remove-Item -LiteralPath $path -Force }
    }
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
    $pinProcess = [System.Diagnostics.Process]::new()
    $pinProcess.StartInfo = $start
    [void]$pinProcess.Start()
    $stdoutTask = $pinProcess.StandardOutput.ReadToEndAsync()
    $stderrTask = $pinProcess.StandardError.ReadToEndAsync()

    $deadline = [datetime]::UtcNow.AddSeconds($TimeoutSec)
    $connected = $false
    while ([datetime]::UtcNow -lt $deadline -and -not $pinProcess.HasExited) {
        try { if (Invoke-Cli @("ping")) { $connected = $true; break } } catch {}
        Start-Sleep -Milliseconds 100
    }
    if (-not $connected) { throw "control plane did not become ready" }
    while ([datetime]::UtcNow -lt $deadline -and -not $pinProcess.HasExited -and
        -not (Test-Path -LiteralPath $ready -PathType Leaf)) {
        Start-Sleep -Milliseconds 50
    }
    if (-not (Test-Path -LiteralPath $ready -PathType Leaf)) { throw "target handshake missing" }

    [void](Invoke-Cli @("script", "run", $plugin))
    $captured = ""
    while ([datetime]::UtcNow -lt $deadline -and -not $captured.Contains("XED_POLICY_READY")) {
        try { $captured = Invoke-Cli @("script", "output") } catch {}
        Start-Sleep -Milliseconds 100
    }
    if (-not $captured.Contains("XED_POLICY_READY")) { throw "XED policy did not become ready: $captured" }
    [System.IO.File]::WriteAllBytes($go, [byte[]]@())

    while ([datetime]::UtcNow -lt $deadline -and -not $pinProcess.HasExited) {
        try { $captured = Invoke-Cli @("script", "output") } catch {}
        Start-Sleep -Milliseconds 100
    }
    if (-not $pinProcess.HasExited) { throw "target did not exit" }
    [void]$pinProcess.WaitForExit()
    if (Test-Path -LiteralPath $log) { $captured += "`n" + (Get-Content -LiteralPath $log -Raw) }
    foreach ($marker in @("XED_DECODE_NAMED_PASS", "XED_DECODE_BATCH_PASS")) {
        if (-not $captured.Contains($marker)) { throw "missing marker $marker`: $captured" }
    }
    if ($captured.Contains("callback failed") -or $captured.Contains("top-level failed")) {
        throw "XED Python plugin failed: $captured"
    }
    $targetOutput = $stdoutTask.Result.Trim()
    $targetError = $stderrTask.Result.Trim()
    if ($pinProcess.ExitCode -ne 0 -or $targetOutput -notmatch "target decoded") {
        throw "target failed: exit=$($pinProcess.ExitCode) stdout=$targetOutput stderr=$targetError"
    }
    [ordered]@{
        result = "XED_DECODE_PYTHON_POLICY_PASS"
        target_exit = $pinProcess.ExitCode
        pre_decode_policy = $true
        native_range_filter = $true
        named_callback = $true
        batch_callback = $true
        target_output = $targetOutput
    } | ConvertTo-Json -Depth 3
} finally {
    if ($null -ne $pinProcess -and -not $pinProcess.HasExited) { $pinProcess.Kill(); $pinProcess.WaitForExit() }
    if ($null -eq $oldPort) { Remove-Item Env:PINBRIDGE_AGENT_PORT -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_PORT = $oldPort }
    if ($null -eq $oldLog) { Remove-Item Env:PINBRIDGE_AGENT_LOG -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_LOG = $oldLog }
    if ($null -eq $oldEngines) { Remove-Item Env:PINBRIDGE_AGENT_ENGINES -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_ENGINES = $oldEngines }
}
