[CmdletBinding()]
param(
    [ValidateSet("x86", "x64")]
    [string]$Arch = "x86",
    [ValidateRange(45, 120)]
    [int]$TimeoutSec = 75
)

$ErrorActionPreference = "Stop"
$fixtureDir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $fixtureDir "..\..")).Path
$bundle = Split-Path -Parent $repo
$rustTarget = Join-Path $repo "bindings\rust\target"
$cli = Join-Path $rustTarget "release\pinbridge-cli.exe"
$fixture = Join-Path $fixtureDir ("hook_syscall_{0}.exe" -f $Arch)
$agent = if ($Arch -eq "x86") {
    Join-Path $rustTarget "i686-pc-windows-msvc\release\pinbridge_agent.dll"
} else {
    Join-Path $rustTarget "release\pinbridge_agent.dll"
}
$pinRoot = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin"
$pin = Join-Path $pinRoot ("{0}\bin\pin.exe" -f $(if ($Arch -eq "x86") { "ia32" } else { "intel64" }))

foreach ($required in @($cli, $fixture, $agent, $pin)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "required file not found: $required"
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

$port = Get-FreePort
$tag = "cli_modify_{0}_{1}" -f $Arch, $port
$agentLog = Join-Path $fixtureDir ("{0}.agent.log" -f $tag)
$targetOut = Join-Path $fixtureDir ("{0}.stdout.txt" -f $tag)
$targetErr = Join-Path $fixtureDir ("{0}.stderr.txt" -f $tag)
$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldLog = $env:PINBRIDGE_AGENT_LOG
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$pinProcess = $null

function Invoke-Cli {
    param([Parameter(Mandatory = $true)][string[]]$Command)
    $args = @("--port", $port.ToString()) + $Command
    $quoted = @($args | ForEach-Object {
        '"' + ([string]$_).Replace('"', '\"') + '"'
    }) -join " "
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
    if (-not $process.WaitForExit(15000)) {
        $process.Kill()
        $process.WaitForExit()
        throw "pinbridge-cli timed out: $($Command -join ' ')"
    }
    if ($process.ExitCode -ne 0) {
        $message = (@($stderr.Result.Trim(), $stdout.Result.Trim()) |
            Where-Object { $_ }) -join "`n"
        throw "pinbridge-cli failed: $($Command -join ' ')`n$message"
    }
    if (-not $stdout.Result.Trim()) { return $null }
    return ($stdout.Result.Trim() | ConvertFrom-Json)
}

$failure = $null
try {
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_LOG = $agentLog
    $env:PINBRIDGE_AGENT_ENGINES = "syscall"

    $start = [System.Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $pin
    $start.Arguments = ('-t "{0}" -- "{1}"' -f $agent, $fixture)
    $start.WorkingDirectory = $fixtureDir
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $pinProcess = [System.Diagnostics.Process]::new()
    $pinProcess.StartInfo = $start
    [void]$pinProcess.Start()
    $pinStdout = $pinProcess.StandardOutput.ReadToEndAsync()
    $pinStderr = $pinProcess.StandardError.ReadToEndAsync()

    $ping = $null
    $ready = [datetime]::UtcNow.AddSeconds(30)
    while ([datetime]::UtcNow -lt $ready) {
        if ($pinProcess.HasExited) {
            throw "Pin exited before ping (exit=$($pinProcess.ExitCode))"
        }
        try { $ping = Invoke-Cli -Command @("ping") } catch { $ping = $null }
        if ($null -ne $ping) { break }
        Start-Sleep -Milliseconds 150
    }
    if ($null -eq $ping) { throw "agent did not answer ping" }

    $exports = Invoke-Cli -Command @("exports", "ntdll.dll")
    $ntclose = @($exports.exports |
        Where-Object { $_.name -eq "NtClose" } | Select-Object -First 1)
    if ($ntclose.Count -ne 1) { throw "NtClose export not found" }
    $address = $ntclose[0].address
    [void](Invoke-Cli -Command @("hook", $address))
    # NtClose is stdcall on x86, so its first argument lives on the stack;
    # Win64 passes it in RCX. The rule parser maps stack0 to [ESP+4] on x86.
    $setRegister = if ($Arch -eq "x86") { "stack0" } else { "rcx" }
    $rule = Invoke-Cli -Command @("hookrule", $address, $setRegister, "0")

    $deadline = [datetime]::UtcNow.AddSeconds($TimeoutSec)
    $counters = $null
    $sawHook = $false
    $maxHookRegs = 0L
    $maxDropped = 0L
    $hookEvent = $null
    while ([datetime]::UtcNow -lt $deadline) {
        if ($pinProcess.HasExited) { break }
        try { $counters = Invoke-Cli -Command @("counters") } catch { $counters = $null }
        if ($null -ne $counters) {
            $maxHookRegs = [Math]::Max($maxHookRegs, [int64]$counters.hook_regs)
            $maxDropped = [Math]::Max($maxDropped, [int64]$counters.dropped)
        }
        if ($null -ne $counters -and [int64]$counters.hook_regs -gt 0) {
            $sawHook = $true
            try {
                $events = Invoke-Cli -Command @("events", "64")
                $hookEvent = @($events.events |
                    Where-Object { [uint64]$_.address -eq [uint64]$address } |
                    Select-Object -Last 1)
                if ($hookEvent.Count -eq 0) { $hookEvent = $null }
            } catch { $hookEvent = $null }
        }
        try {
            if ($null -eq (Get-Process -Id ([int]$ping.pid) -ErrorAction SilentlyContinue)) { break }
        } catch { break }
        Start-Sleep -Milliseconds 250
    }
    $targetGone = $false
    try {
        $targetGone = $null -eq (Get-Process -Id ([int]$ping.pid) -ErrorAction SilentlyContinue)
    } catch { $targetGone = $false }
    if (-not $targetGone -and -not $pinProcess.HasExited) {
        throw "target did not exit within $TimeoutSec seconds"
    }
    if (-not $pinProcess.HasExited) {
        [void]$pinProcess.WaitForExit(10000)
    }
    if (-not $pinProcess.HasExited) {
        $pinProcess.Kill()
        $pinProcess.WaitForExit()
    }
    [System.IO.File]::WriteAllText($targetOut, $pinStdout.Result)
    [System.IO.File]::WriteAllText($targetErr, $pinStderr.Result)
    $targetOutput = $pinStdout.Result.Trim()
    if (-not $sawHook) {
        throw "target completed without a Hook event"
    }
    if ($targetOutput -notmatch "close=0xc0000008") {
        throw "hook rule did not change NtClose result: $targetOutput"
    }

    [ordered]@{
        arch = $Arch
        pointer_width = if ($Arch -eq "x86") { 4 } else { 8 }
        pid = $ping.pid
        address = $address
        set_register = $setRegister
        rule_ok = [bool]$rule.ok
        original_stack0 = if ($null -ne $hookEvent) { $hookEvent.arg4 } else { $null }
        hook_regs = $maxHookRegs
        dropped = $maxDropped
        target_output = $targetOutput
        agent_log = $agentLog
    } | ConvertTo-Json -Depth 5
} catch {
    $failure = $_.Exception.ToString()
    Write-Error $failure
    throw
} finally {
    if ($null -ne $failure) {
        try { [System.IO.File]::WriteAllText((Join-Path $fixtureDir ("{0}.error.txt" -f $tag)), $failure) } catch {}
    }
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
}
