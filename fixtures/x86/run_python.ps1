[CmdletBinding()]
param(
    [ValidateRange(20, 120)]
    [int]$TimeoutSec = 60
)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$target = Join-Path $dir "hello32.exe"
$plugin = Join-Path $dir "python_probe.py"
$cli = Join-Path $repo "bindings\rust\target\release\pinbridge-cli.exe"
$agent = Join-Path $repo "bindings\rust\target\i686-pc-windows-msvc\release\pinbridge_agent.dll"
$pythonDll = Join-Path (Split-Path -Parent $agent) "python310.dll"
$pythonZip = Join-Path (Split-Path -Parent $agent) "python310.zip"
$pin = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\ia32\bin\pin.exe"

foreach ($path in @($target, $plugin, $cli, $agent, $pythonDll, $pythonZip, $pin)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required x86 Python test file not found: $path"
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
$log = Join-Path $dir ("x86_python_{0}.agent.log" -f $port)
$saved = @{
    Port = $env:PINBRIDGE_AGENT_PORT
    Log = $env:PINBRIDGE_AGENT_LOG
    Engines = $env:PINBRIDGE_AGENT_ENGINES
    Entry = $env:PINBRIDGE_ENTRY_BP
}
$pinProcess = $null

try {
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_LOG = $log
    $env:PINBRIDGE_AGENT_ENGINES = "none"
    $env:PINBRIDGE_ENTRY_BP = "1"
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
        try { if (Invoke-Cli -Command @("ping")) { $connected = $true; break } } catch {}
        Start-Sleep -Milliseconds 100
    }
    if (-not $connected) { throw "x86 control plane did not become ready" }

    $loaded = $false
    while ([datetime]::UtcNow -lt $deadline -and -not $loaded) {
        try {
            [void](Invoke-Cli -Command @("script", "run", $plugin))
            $loaded = $true
        } catch {
            Start-Sleep -Milliseconds 100
        }
    }
    if (-not $loaded) { throw "x86 Python plugin did not compile" }

    $captured = ""
    while ([datetime]::UtcNow -lt $deadline -and
           -not $captured.Contains("X86_PYTHON_READY")) {
        try { $captured = Invoke-Cli -Command @("script", "output") } catch {}
        Start-Sleep -Milliseconds 50
    }
    if (-not $captured.Contains("X86_PYTHON_READY")) {
        throw "x86 Python plugin did not initialize"
    }

    [void](Invoke-Cli -Command @("resume"))
    while ([datetime]::UtcNow -lt $deadline -and -not $pinProcess.HasExited) {
        try { $captured = Invoke-Cli -Command @("script", "output") } catch {}
        if ($captured.Contains("callback failed")) {
            throw "x86 Python breakpoint callback reported an error"
        }
        Start-Sleep -Milliseconds 50
    }
    if (-not $pinProcess.HasExited) { throw "x86 target did not exit" }
    [void]$pinProcess.WaitForExit()
    if (Test-Path -LiteralPath $log) {
        $captured += "`n" + (Get-Content -LiteralPath $log -Raw -Encoding UTF8)
    }
    if (-not $captured.Contains("X86_PYTHON_BREAKPOINT_PASS")) {
        throw "x86 exact Python breakpoint callback did not run"
    }
    $targetOutput = $stdoutTask.Result.Trim()
    $targetError = $stderrTask.Result.Trim()
    if ($pinProcess.ExitCode -ne 0 -or $targetOutput -notmatch "hello32: input=pinbridge-ia32") {
        throw "x86 target failed: exit=$($pinProcess.ExitCode) stdout=$targetOutput stderr=$targetError"
    }
    [ordered]@{
        result = "X86_PYTHON_BREAKPOINT_PASS"
        target_exit = $pinProcess.ExitCode
        control_port = $port
        python_initialized = $true
        exact_breakpoint_callback = $true
        target_output = $targetOutput
    } | ConvertTo-Json -Depth 3
} finally {
    if ($null -ne $pinProcess -and -not $pinProcess.HasExited) {
        $pinProcess.Kill()
        $pinProcess.WaitForExit()
    }
    foreach ($entry in @(
        @("PINBRIDGE_AGENT_PORT", $saved.Port),
        @("PINBRIDGE_AGENT_LOG", $saved.Log),
        @("PINBRIDGE_AGENT_ENGINES", $saved.Engines),
        @("PINBRIDGE_ENTRY_BP", $saved.Entry)
    )) {
        if ($null -eq $entry[1]) {
            Remove-Item ("Env:" + $entry[0]) -ErrorAction SilentlyContinue
        } else {
            Set-Item ("Env:" + $entry[0]) $entry[1]
        }
    }
}
