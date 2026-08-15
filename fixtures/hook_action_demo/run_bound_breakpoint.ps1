[CmdletBinding()]
param(
    [ValidateRange(45, 120)]
    [int]$TimeoutSec = 75
)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$target = Join-Path $dir "hook_action_demo_x64.exe"
$scriptPath = Join-Path $dir "bound_breakpoint.py"
$cli = Join-Path $repo "bindings\rust\target\release\pinbridge-cli.exe"
$agent = Join-Path $repo "bindings\rust\target\release\pinbridge_agent.dll"
$pin = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\intel64\bin\pin.exe"

foreach ($path in @($target, $scriptPath, $cli, $agent, $pin)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required file not found: $path"
    }
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new(
        [System.Net.IPAddress]::Loopback, 0)
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
$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$pinProcess = $null

try {
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_ENGINES = "syscall"
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
    while ([datetime]::UtcNow -lt $deadline) {
        if ($pinProcess.HasExited) { throw "Pin exited before control plane was ready" }
        try {
            if (Invoke-Cli -Command @("ping")) { break }
        } catch {}
        Start-Sleep -Milliseconds 150
    }
    if ([datetime]::UtcNow -ge $deadline) { throw "control plane did not become ready" }

    [void](Invoke-Cli -Command @("script", "run", $scriptPath))
    $ready = $false
    $hit = $false
    while ([datetime]::UtcNow -lt $deadline -and -not $pinProcess.HasExited) {
        try {
            $output = Invoke-Cli -Command @("script", "output")
            $ready = $ready -or $output.Contains("BOUND_BP_READY")
            $hit = $hit -or $output.Contains("BOUND_BP_HIT")
        } catch {}
        if ($hit) { break }
        Start-Sleep -Milliseconds 200
    }
    if (-not $ready) { throw "bound breakpoint was not registered" }
    if (-not $hit) { throw "bound breakpoint callback was not delivered" }

    while (-not $pinProcess.HasExited -and [datetime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 100
    }
    if (-not $pinProcess.HasExited) { throw "target did not exit" }
    [void]$pinProcess.WaitForExit()
    $targetOutput = $stdoutTask.Result.Trim()
    $targetError = $stderrTask.Result.Trim()
    if ($targetOutput -notmatch "hooked=4660") {
        throw "callback did not modify the stopped context: $targetOutput $targetError"
    }
    [ordered]@{
        result = "BOUND_BREAKPOINT_PASS"
        port = $port
        target_exit = $pinProcess.ExitCode
        callback_seen = $hit
        target_output = $targetOutput
    } | ConvertTo-Json -Depth 3
} finally {
    if ($null -ne $pinProcess -and -not $pinProcess.HasExited) {
        $pinProcess.Kill()
        $pinProcess.WaitForExit()
    }
    if ($null -eq $oldPort) {
        Remove-Item Env:PINBRIDGE_AGENT_PORT -ErrorAction SilentlyContinue
    } else {
        $env:PINBRIDGE_AGENT_PORT = $oldPort
    }
    if ($null -eq $oldEngines) {
        Remove-Item Env:PINBRIDGE_AGENT_ENGINES -ErrorAction SilentlyContinue
    } else {
        $env:PINBRIDGE_AGENT_ENGINES = $oldEngines
    }
}
