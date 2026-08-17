[CmdletBinding()]
param(
    [string]$Target = "D:\vmp_trace_toolkit_bundle_20260801_clean\analysis\live-unpack-20260817\crypto.vmp.exe",
    [UInt64]$ExpectedOep = 0x140008B70,
    [ValidateRange(15, 180)][int]$TimeoutSec = 90
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$bundle = Split-Path -Parent $repo
$plugin = Join-Path $repo "examples\python\vmp_oep.py"
$cli = Join-Path $repo "bindings\rust\target\release\pinbridge-cli.exe"
$agent = Join-Path $repo "bindings\rust\target\release\pinbridge_agent.dll"
$pin = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\intel64\bin\pin.exe"

foreach ($path in @($Target, $plugin, $cli, $agent, $pin)) {
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
$log = Join-Path $PSScriptRoot ("vmp_oep_live_{0}.agent.log" -f $port)
$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldLog = $env:PINBRIDGE_AGENT_LOG
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$oldEntry = $env:PINBRIDGE_ENTRY_BP
$process = $null

try {
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_LOG = $log
    $env:PINBRIDGE_AGENT_ENGINES = "none"
    $env:PINBRIDGE_ENTRY_BP = "1"
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $pin
    $start.Arguments = '-t "{0}" -- "{1}"' -f $agent, (Resolve-Path -LiteralPath $Target).Path
    $start.WorkingDirectory = Split-Path -Parent (Resolve-Path -LiteralPath $Target).Path
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

    $connected = $false
    while ([datetime]::UtcNow -lt $deadline -and -not $process.HasExited) {
        try { [void](Invoke-Cli @("ping")); $connected = $true; break } catch { Start-Sleep -Milliseconds 50 }
    }
    if (-not $connected) { throw "control plane did not become ready" }

    $entryStopped = $false
    while ([datetime]::UtcNow -lt $deadline -and -not $entryStopped) {
        try {
            $state = Invoke-Cli @("bps") | ConvertFrom-Json
            $entryStopped = [bool]$state.stopped
        } catch {}
        if (-not $entryStopped) { Start-Sleep -Milliseconds 50 }
    }
    if (-not $entryStopped) { throw "launcher entry breakpoint did not stop the target" }

    [void](Invoke-Cli @("script", "run", $plugin))
    $captured = ""
    while ([datetime]::UtcNow -lt $deadline -and -not $captured.Contains("vmp_oep: ready")) {
        $captured = Invoke-Cli @("script", "output")
        $plugins = Invoke-Cli @("script", "list") | ConvertFrom-Json
        $failed = @($plugins.plugins | Where-Object { $_.name -eq "vmp_oep.py" -and [int]$_.state -eq 2 }).Count -ne 0
        if ($failed) { throw "VMP OEP strategy initialization failed: $captured" }
        Start-Sleep -Milliseconds 50
    }
    if (-not $captured.Contains("vmp_oep: ready")) { throw "VMP OEP strategy did not initialize" }
    [void](Invoke-Cli @("resume"))

    while ([datetime]::UtcNow -lt $deadline -and -not $captured.Contains("vmp_oep: HIT candidate")) {
        if ($process.HasExited) { break }
        try { $captured = Invoke-Cli @("script", "output") } catch {}
        Start-Sleep -Milliseconds 50
    }
    if (-not $captured.Contains("vmp_oep: HIT candidate")) {
        throw "OEP was not captured before timeout/target exit"
    }
    $match = [regex]::Match($captured, 'HIT candidate VA=0x([0-9a-fA-F]+) RVA=0x([0-9a-fA-F]+)')
    if (-not $match.Success) { throw "OEP hit output is malformed" }
    $actual = [Convert]::ToUInt64($match.Groups[1].Value, 16)
    $rva = [Convert]::ToUInt64($match.Groups[2].Value, 16)
    if ($ExpectedOep -ne 0 -and $actual -ne $ExpectedOep) {
        throw ('wrong OEP: actual=0x{0:x} expected=0x{1:x}' -f $actual, $ExpectedOep)
    }
    $state = Invoke-Cli @("bps") | ConvertFrom-Json
    if (-not [bool]$state.stopped) { throw "target was not left stopped at the OEP" }
    [ordered]@{
        result = "VMP_OEP_CAPTURE_PASS"
        va = ('0x{0:x}' -f $actual)
        rva = ('0x{0:x}' -f $rva)
        stopped = [bool]$state.stopped
        strategy = "external-python-late-arm"
        primitive = "generic-native-execution-range-trap"
    } | ConvertTo-Json
} finally {
    if ($null -ne $process -and -not $process.HasExited) {
        & taskkill.exe /PID $process.Id /T /F 2>$null | Out-Null
        try { $process.WaitForExit(5000) | Out-Null } catch {}
    }
    if ($null -eq $oldPort) { Remove-Item Env:PINBRIDGE_AGENT_PORT -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_PORT = $oldPort }
    if ($null -eq $oldLog) { Remove-Item Env:PINBRIDGE_AGENT_LOG -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_LOG = $oldLog }
    if ($null -eq $oldEngines) { Remove-Item Env:PINBRIDGE_AGENT_ENGINES -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_ENGINES = $oldEngines }
    if ($null -eq $oldEntry) { Remove-Item Env:PINBRIDGE_ENTRY_BP -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_ENTRY_BP = $oldEntry }
}
