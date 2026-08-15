[CmdletBinding()]
param(
    [bool]$Follow = $false,
    [ValidateRange(30, 120)]
    [int]$TimeoutSec = 60
)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$target = Join-Path $dir "child_follow_demo_x64.exe"
$plugin = Join-Path $dir "child_decision.py"
$childPlugin = Join-Path $dir "child_session.py"
$cli = Join-Path $repo "bindings\rust\target\release\pinbridge-cli.exe"
$agent = Join-Path $repo "bindings\rust\target\release\pinbridge_agent.dll"
$pin = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\intel64\bin\pin.exe"

foreach ($path in @($target, $plugin, $childPlugin, $cli, $agent, $pin)) {
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
    param(
        [Parameter(Mandatory = $true)][string[]]$Command,
        [int]$Port = $script:Port
    )
    $arguments = @("--port", $Port.ToString()) + $Command
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
$log = Join-Path $dir ("child_follow_{0}_{1}.agent.log" -f $Follow, $port)
$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldLog = $env:PINBRIDGE_AGENT_LOG
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$oldFollow = $env:PINBRIDGE_TEST_FOLLOW_CHILD
$pinProcess = $null
$childPid = 0

try {
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_LOG = $log
    $env:PINBRIDGE_AGENT_ENGINES = "none"
    $env:PINBRIDGE_TEST_FOLLOW_CHILD = $(if ($Follow) { "1" } else { "0" })
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $pin
    $start.Arguments = '-follow_execv -t "{0}" -- "{1}"' -f $agent, $target
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

    $childPort = 0
    $childOutput = ""
    if ($Follow) {
        $decisionOutput = ""
        while ([datetime]::UtcNow -lt $deadline -and $childPort -eq 0) {
            try { $decisionOutput = Invoke-Cli -Command @("script", "output") } catch {}
            if ($decisionOutput -match
                'CHILD_DECISION_PASS pid=(\d+) follow=True control_port=(\d+) parent_port=(\d+)') {
                $childPid = [int]$Matches[1]
                $childPort = [int]$Matches[2]
                $reportedParentPort = [int]$Matches[3]
                if ($childPort -eq $port -or $reportedParentPort -ne $port) {
                    throw "child control topology was not independent"
                }
                break
            }
            Start-Sleep -Milliseconds 50
        }
        if ($childPort -eq 0) { throw "Python decision did not publish the child control port" }

        $childConnected = $false
        while ([datetime]::UtcNow -lt $deadline -and -not $childConnected) {
            try {
                if (Invoke-Cli -Port $childPort -Command @("ping")) {
                    $childConnected = $true
                }
            } catch {}
            if (-not $childConnected) { Start-Sleep -Milliseconds 50 }
        }
        if (-not $childConnected) { throw "followed child control plane did not become ready" }
        $childLoaded = $false
        while ([datetime]::UtcNow -lt $deadline -and -not $childLoaded) {
            try {
                [void](Invoke-Cli -Port $childPort -Command @("script", "run", $childPlugin))
                $childLoaded = $true
            } catch {
                Start-Sleep -Milliseconds 50
            }
        }
        if (-not $childLoaded) { throw "child Python plugin did not load" }
        while ([datetime]::UtcNow -lt $deadline -and
               -not $childOutput.Contains("CHILD_SESSION_PYTHON_PASS")) {
            try {
                $childOutput = Invoke-Cli -Port $childPort -Command @("script", "output")
            } catch {}
            Start-Sleep -Milliseconds 50
        }
        if (-not $childOutput.Contains("CHILD_SESSION_PYTHON_PASS")) {
            throw "Python did not execute through the child control plane"
        }
    }

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

    foreach ($marker in @("CHILD_DECISION_READY", "CHILD_DECISION_PASS")) {
        if (-not $captured.Contains($marker)) { throw "missing callback marker: $marker" }
    }
    $expectedCounts = if ($Follow) {
        "child_decisions=1 child_follow=1 child_reject=0"
    } else {
        "child_decisions=1 child_follow=0 child_reject=1"
    }
    if (-not $captured.Contains($expectedCounts)) {
        throw "native child-follow result was not observed: expected $expectedCounts"
    }
    if (-not $captured.Contains("child_config_failures=0")) {
        throw "child Pin command-line configuration failed"
    }
    $targetOutput = $stdoutTask.Result.Trim()
    $targetError = $stderrTask.Result.Trim()
    if ($pinProcess.ExitCode -ne 0 -or $targetOutput -notmatch "parent child_exit=") {
        throw "target failed: exit=$($pinProcess.ExitCode) stdout=$targetOutput stderr=$targetError"
    }
    [ordered]@{
        result = "CHILD_FOLLOW_DECISION_PASS"
        requested_follow = $Follow
        target_exit = $pinProcess.ExitCode
        callback = $true
        child_control_port = $(if ($Follow) { $childPort } else { $null })
        child_python = $(if ($Follow) { $true } else { $false })
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
    if ($null -eq $oldFollow) { Remove-Item Env:PINBRIDGE_TEST_FOLLOW_CHILD -ErrorAction SilentlyContinue }
    else { $env:PINBRIDGE_TEST_FOLLOW_CHILD = $oldFollow }
    if ($childPid -ne 0) {
        $childReady = Join-Path $dir ("child_control_{0}.ready" -f $childPid)
        Remove-Item -LiteralPath $childReady -Force -ErrorAction SilentlyContinue
    }
}
