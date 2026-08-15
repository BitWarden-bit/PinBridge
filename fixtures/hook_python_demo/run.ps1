[CmdletBinding()]
param(
    [ValidateRange(30, 120)]
    [int]$TimeoutSec = 60
)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$target = Join-Path $dir "hook_python_demo_x64.exe"
$plugin = Join-Path $dir "hook_intercept.py"
$failurePlugin = Join-Path $dir "hook_init_failure.py"
$cli = Join-Path $repo "bindings\rust\target\release\pinbridge-cli.exe"
$agent = Join-Path $repo "bindings\rust\target\release\pinbridge_agent.dll"
$pin = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\intel64\bin\pin.exe"

foreach ($path in @($target, $plugin, $failurePlugin, $cli, $agent, $pin)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required file not found: $path"
    }
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    } finally {
        $listener.Stop()
    }
}

function Invoke-Cli {
    param([Parameter(Mandatory = $true)][string[]]$Command)
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
    if (-not $process.WaitForExit(15000)) {
        $process.Kill()
        $process.WaitForExit()
        throw "pinbridge-cli timed out"
    }
    if ($process.ExitCode -ne 0) {
        throw "pinbridge-cli failed: $($stderr.Result.Trim()) $($stdout.Result.Trim())"
    }
    return $stdout.Result.Trim()
}

$port = Get-FreePort
$script:Port = $port
$script:Cli = $cli
$log = Join-Path $dir ("hook_python_{0}.agent.log" -f $port)
$registrationReady = Join-Path $dir "hook_registration.ready"
$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldLog = $env:PINBRIDGE_AGENT_LOG
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$pinProcess = $null

try {
    Remove-Item -LiteralPath $registrationReady -Force -ErrorAction SilentlyContinue
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
        try {
            if (Invoke-Cli -Command @("ping")) { $connected = $true; break }
        } catch {}
        Start-Sleep -Milliseconds 100
    }
    if (-not $connected) { throw "control plane did not become ready" }

    # A failed plugin must remain diagnosable as state=2 while every native
    # breakpoint and both Hook leases it acquired are released before the
    # target is allowed to execute the tested functions.
    [void](Invoke-Cli -Command @("script", "run", $failurePlugin))
    $failureQuarantined = $false
    while ([datetime]::UtcNow -lt $deadline -and -not $failureQuarantined) {
        try {
            $plugins = Invoke-Cli -Command @("script", "list") | ConvertFrom-Json
            $failed = @($plugins.plugins | Where-Object {
                $_.name -eq "hook_init_failure.py" -and [int]$_.state -eq 2
            }).Count -eq 1
            $hooks = Invoke-Cli -Command @("hooks") | ConvertFrom-Json
            $breakpoints = Invoke-Cli -Command @("bps") | ConvertFrom-Json
            $failureQuarantined = $failed -and [int]$hooks.count -eq 0 -and
                [int]$breakpoints.count -eq 0
        } catch {}
        if (-not $failureQuarantined) { Start-Sleep -Milliseconds 50 }
    }
    if (-not $failureQuarantined) {
        throw "failed plugin retained native Hook or breakpoint resources"
    }
    $failureOutput = Invoke-Cli -Command @("script", "output")
    if (-not $failureOutput.Contains("HOOK_INIT_FAILURE_ARMED") -or
        -not $failureOutput.Contains("released 1 breakpoints and 2 Hook leases")) {
        throw "failed-plugin quarantine diagnostics were incomplete"
    }

    $loaded = $false
    while ([datetime]::UtcNow -lt $deadline -and -not $loaded) {
        try {
            [void](Invoke-Cli -Command @("script", "run", $plugin))
            $loaded = $true
        } catch {
            Start-Sleep -Milliseconds 150
        }
    }
    if (-not $loaded) { throw "Python plugin did not load" }

    $captured = ""
    while ([datetime]::UtcNow -lt $deadline -and -not $pinProcess.HasExited) {
        try { $captured = Invoke-Cli -Command @("script", "output") } catch {}
        Start-Sleep -Milliseconds 100
    }
    if (-not $pinProcess.HasExited) { throw "target did not exit" }
    [void]$pinProcess.WaitForExit()
    if (Test-Path -LiteralPath $log) {
        $captured += "`n" + (Get-Content -LiteralPath $log -Raw)
    }
    foreach ($marker in @(
        "HOOK_INTERCEPT_READY",
        "HOOK_ENTRY_INTERCEPT_PASS",
        "HOOK_RETURN_INTERCEPT_PASS",
        "HOOK_ENTRY_OBSERVE_PASS",
        "HOOK_RETURN_OBSERVE_PASS",
        "HOOK_OBSERVE_EXACT_ONCE"
    )) {
        if (-not $captured.Contains($marker)) { throw "missing callback marker: $marker" }
    }
    if (-not $captured.Contains("sync_decisions=3 sync_timeouts=0 sync_busy=0")) {
        throw "native synchronous Hook counters did not confirm three decisions"
    }
    if (-not $captured.Contains("observation_dropped=0")) {
        throw "filtered Hook observation lane dropped events"
    }
    $targetOutput = $stdoutTask.Result.Trim()
    $targetError = $stderrTask.Result.Trim()
    if ($pinProcess.ExitCode -ne 0 -or $targetOutput -notmatch "skipped=4660/4660 returned=22136/17 calls=0/2") {
        throw "target failed: exit=$($pinProcess.ExitCode) stdout=$targetOutput stderr=$targetError"
    }
    [ordered]@{
        result = "HOOK_PYTHON_INTERCEPT_PASS"
        target_exit = $pinProcess.ExitCode
        sync_decisions = 3
        named_observers_exact_once = $true
        observation_dropped = 0
        failed_plugin_quarantined = $true
        skipped_original = $true
        target_output = $targetOutput
    } | ConvertTo-Json -Depth 3
} finally {
    if ($null -ne $pinProcess -and -not $pinProcess.HasExited) {
        $pinProcess.Kill()
        $pinProcess.WaitForExit()
    }
    if ($null -eq $oldPort) { Remove-Item Env:PINBRIDGE_AGENT_PORT -ErrorAction SilentlyContinue }
    else { $env:PINBRIDGE_AGENT_PORT = $oldPort }
    if ($null -eq $oldLog) { Remove-Item Env:PINBRIDGE_AGENT_LOG -ErrorAction SilentlyContinue }
    else { $env:PINBRIDGE_AGENT_LOG = $oldLog }
    if ($null -eq $oldEngines) { Remove-Item Env:PINBRIDGE_AGENT_ENGINES -ErrorAction SilentlyContinue }
    else { $env:PINBRIDGE_AGENT_ENGINES = $oldEngines }
    Remove-Item -LiteralPath $registrationReady -Force -ErrorAction SilentlyContinue
}
