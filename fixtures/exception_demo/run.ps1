[CmdletBinding()]
param(
    [ValidateSet("x86", "x64")]
    [string]$Arch = "x64",
    [ValidateRange(10, 120)]
    [int]$TimeoutSec = 30,
    [switch]$Takeover
)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$target = Join-Path $dir ("exception_demo_{0}.exe" -f $Arch)
$cli = Join-Path $repo "bindings\rust\target\release\pinbridge-cli.exe"
$agent = if ($Arch -eq "x86") {
    Join-Path $repo "bindings\rust\target\i686-pc-windows-msvc\release\pinbridge_agent.dll"
} else {
    Join-Path $repo "bindings\rust\target\release\pinbridge_agent.dll"
}
$pin = Join-Path $bundle ("VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\{0}\bin\pin.exe" -f $(if ($Arch -eq "x86") { "ia32" } else { "intel64" }))

foreach ($path in @($target, $cli, $agent, $pin)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "required file not found: $path" }
}

function Get-FreePort {
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    try { $listener.Start(); return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port }
    finally { $listener.Stop() }
}

$port = Get-FreePort
$tag = "exception_demo_{0}_{1}" -f $Arch, $port
$log = Join-Path $dir ("{0}.agent.log" -f $tag)
$outPath = Join-Path $dir ("{0}.stdout.txt" -f $tag)
$errPath = Join-Path $dir ("{0}.stderr.txt" -f $tag)
$pinProcess = $null
$expectedCodes = @([uint64]3221225477, [uint64]2147483651, [uint64]3221225620)

function Invoke-Cli([string[]]$Command) {
    $args = @("--port", $port.ToString()) + $Command
    $quoted = @($args | ForEach-Object { '"' + ([string]$_).Replace('"', '\"') + '"' }) -join " "
    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $cli
    $start.Arguments = $quoted
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
    if ($process.ExitCode -ne 0) {
        $message = (@($stderr.Result.Trim(), $stdout.Result.Trim()) | Where-Object { $_ }) -join "`n"
        throw "pinbridge-cli $($Command -join ' ') failed: $message"
    }
    if (-not $stdout.Result.Trim()) { return $null }
    return $stdout.Result.Trim() | ConvertFrom-Json
}

try {
    $oldPort = $env:PINBRIDGE_AGENT_PORT
    $oldLog = $env:PINBRIDGE_AGENT_LOG
    $oldEngines = $env:PINBRIDGE_AGENT_ENGINES
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_LOG = $log
    # Start with syscall only; it is disabled immediately after ping. This
    # avoids flooding the ring while retaining the normal production startup.
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

    $ready = [datetime]::UtcNow.AddSeconds(20)
    while ([datetime]::UtcNow -lt $ready) {
        if ($pinProcess.HasExited) { throw "Pin exited before agent became ready" }
        try { if ($null -ne (Invoke-Cli @("ping"))) { break } } catch {}
        Start-Sleep -Milliseconds 100
    }
    if ([datetime]::UtcNow -ge $ready) { throw "agent did not answer ping" }
    [void](Invoke-Cli @("engine", "5", "off"))
    [void](Invoke-Cli @("engine", "2", "off"))
    [void](Invoke-Cli @("engine", "3", "off"))
    [void](Invoke-Cli @("engine", "4", "off"))
    if ($Takeover) {
        $policy = Invoke-Cli @("exc", "all")
        if (-not $policy.enabled -or [uint64]$policy.exception_code -ne 0) {
            throw "failed to enable exception takeover policy"
        }
    }

    $deadline = [datetime]::UtcNow.AddSeconds($TimeoutSec)
    $exceptionEvents = @()
    $takeoverPauses = 0
    $lastStopGen = [uint64]0
    while ([datetime]::UtcNow -lt $deadline) {
        if ($pinProcess.HasExited) { break }
        try {
            if ($Takeover) {
                $state = Invoke-Cli @("bps")
                $stopGen = [uint64]$state.stop_gen
                if ($state.stopped -and $stopGen -gt $lastStopGen) {
                    $resumed = Invoke-Cli @("resume")
                    if (-not $resumed.resumed) { throw "failed to resume exception stop" }
                    $lastStopGen = $stopGen
                    $takeoverPauses++
                }
            }
            $page = Invoke-Cli @("events", "4096")
            $exceptionEvents = @($page.events | Where-Object {
                [string]$_.kind_name -eq "context_change" -and
                ($expectedCodes -contains ([uint64]$_.arg1 -band [uint64]4294967295))
            })
            $codes = @($exceptionEvents | ForEach-Object { [uint64]$_.arg1 -band [uint64]4294967295 })
            $eventsReady = (($codes -contains [uint64]3221225477) -and
                ($codes -contains [uint64]2147483651) -and
                ($codes -contains [uint64]3221225620) -and
                (@($codes | Where-Object { $_ -eq [uint64]3221225477 }).Count -ge 2))
            if ($eventsReady -and (-not $Takeover -or $takeoverPauses -ge 4)) { break }
        } catch {}
        Start-Sleep -Milliseconds 100
    }

    $codes = @($exceptionEvents | ForEach-Object { [uint64]$_.arg1 -band [uint64]4294967295 })
    if ((@($codes | Where-Object { $_ -eq [uint64]3221225477 }).Count -lt 2) -or
        -not ($codes -contains [uint64]2147483651) -or
        -not ($codes -contains [uint64]3221225620)) {
        throw "expected AV/INT3/DIV0 context_change events, found $($exceptionEvents.Count)"
    }
    if ($Takeover -and $takeoverPauses -lt 4) {
        throw "expected four exception takeover pauses, found $takeoverPauses"
    }
    foreach ($event in $exceptionEvents) {
        if ([uint64]$event.arg0 -ne 4) { throw "unexpected context-change reason: $($event.arg0)" }
        $code = [uint64]$event.arg1 -band [uint64]4294967295
        if (-not ($expectedCodes -contains $code)) { throw "unexpected exception code: $($event.arg1)" }
        if ([uint64]$event.arg2 -eq 0) { throw "exception IP is zero" }
        if ([uint64]$event.address -ne [uint64]$event.arg2) { throw "context-change address does not match exception IP" }
        if ([uint64]$event.thread_id -eq [uint64]4294967295) { throw "exception thread id is invalid" }
    }

    while (-not $pinProcess.HasExited -and [datetime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if (-not $pinProcess.HasExited) { throw "target did not exit within $TimeoutSec seconds" }
    [void]$pinProcess.WaitForExit()
    [System.IO.File]::WriteAllText($outPath, $stdoutTask.Result)
    [System.IO.File]::WriteAllText($errPath, $stderrTask.Result)
    if ($pinProcess.ExitCode -ne 0) { throw "Pin/target exited with code $($pinProcess.ExitCode)" }
    $targetOutput = $stdoutTask.Result.Trim()
    if ($targetOutput -notmatch "av=1 bp=1 div0=1" -or
        $targetOutput -notmatch "handled=1 total=4 veh=\d+") {
        throw "unexpected target output: $targetOutput"
    }

    [ordered]@{
        arch = $Arch
        exception_codes = @("0xC0000005", "0x80000003", "0xC0000094")
        events = $exceptionEvents.Count
        takeover = [bool]$Takeover
        takeover_pauses = $takeoverPauses
        first_thread_id = [uint32]$exceptionEvents[0].thread_id
        first_exception_ip = ('0x{0:x}' -f [uint64]$exceptionEvents[0].arg2)
        target_output = $targetOutput
        agent_log = $log
        stdout = $outPath
        stderr = $errPath
    } | ConvertTo-Json -Depth 4
} finally {
    if ($null -ne $pinProcess -and -not $pinProcess.HasExited) { $pinProcess.Kill(); $pinProcess.WaitForExit() }
    if ($null -eq $oldPort) { Remove-Item Env:PINBRIDGE_AGENT_PORT -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_PORT = $oldPort }
    if ($null -eq $oldLog) { Remove-Item Env:PINBRIDGE_AGENT_LOG -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_LOG = $oldLog }
    if ($null -eq $oldEngines) { Remove-Item Env:PINBRIDGE_AGENT_ENGINES -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_ENGINES = $oldEngines }
}
