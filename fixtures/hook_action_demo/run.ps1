[CmdletBinding()]
param(
    [ValidateSet("x86", "x64")]
    [string]$Arch = "x64",
    [ValidateRange(45, 120)]
    [int]$TimeoutSec = 75
)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$target = Join-Path $dir ("hook_action_demo_{0}.exe" -f $Arch)
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
$tag = "hook_action_{0}_{1}" -f $Arch, $port
$log = Join-Path $dir ("{0}.agent.log" -f $tag)
$outPath = Join-Path $dir ("{0}.stdout.txt" -f $tag)
$errPath = Join-Path $dir ("{0}.stderr.txt" -f $tag)
$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldLog = $env:PINBRIDGE_AGENT_LOG
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$pinProcess = $null

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

$failurePath = Join-Path $dir ("{0}.error.txt" -f $tag)
try {
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_LOG = $log
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

    $ready = [datetime]::UtcNow.AddSeconds(30)
    while ([datetime]::UtcNow -lt $ready) {
        if ($pinProcess.HasExited) { throw "Pin exited before agent became ready" }
        try { if ($null -ne (Invoke-Cli @("ping"))) { break } } catch {}
        Start-Sleep -Milliseconds 150
    }
    if ([datetime]::UtcNow -ge $ready) { throw "agent did not answer ping" }
    [void](Invoke-Cli @("engine", "5", "off"))

    $modules = Invoke-Cli @("modules")
    $main = @($modules.modules | Where-Object { $_.main } | Select-Object -First 1)
    if ($main.Count -ne 1) { throw "main module not found" }
    $mainName = Split-Path -Leaf ([string]$main[0].name)
    $exports = Invoke-Cli @("exports", $mainName)
    $api = @($exports.exports | Where-Object { $_.name -eq "DemoApi" } | Select-Object -First 1)
    if ($api.Count -ne 1) { throw "DemoApi export not found" }
    $entry = [string]$api[0].address
    $disasm = Invoke-Cli @("disasm", $entry, "32")
    $returnRow = @($disasm.insns | Where-Object { $_.text -match '^\s*ret' } | Select-Object -First 1)
    if ($returnRow.Count -ne 1) { throw "DemoApi ret instruction not found" }
    $returnAddress = [string]$returnRow[0].address
    [void](Invoke-Cli @("hook", $entry))
    [void](Invoke-Cli @("hook", $returnAddress))
    $argRegister = if ($Arch -eq "x86") { "stack0" } else { "rcx" }
    $returnRegister = if ($Arch -eq "x86") { "eax" } else { "rax" }
    $argRule = Invoke-Cli @("hookrule", $entry, $argRegister, "20")
    $returnRule = Invoke-Cli @("hookrule", $returnAddress, $returnRegister, "0x1234")

    $deadline = [datetime]::UtcNow.AddSeconds($TimeoutSec)
    $entryEvent = $null
    $returnEvent = $null
    while ([datetime]::UtcNow -lt $deadline -and ($null -eq $entryEvent -or $null -eq $returnEvent)) {
        if ($pinProcess.HasExited) { break }
        try {
            $events = Invoke-Cli @("events", "64")
            $entryEvent = @($events.events | Where-Object {
                [string]$_.kind_name -eq "hook_regs" -and [uint64]$_.address -eq [uint64]$entry
            } | Select-Object -Last 1)
            if ($entryEvent.Count -eq 0) { $entryEvent = $null }
            $returnEvent = @($events.events | Where-Object {
                [string]$_.kind_name -eq "hook_return" -and [uint64]$_.address -eq [uint64]$returnAddress
            } | Select-Object -Last 1)
            if ($returnEvent.Count -eq 0) { $returnEvent = $null }
        } catch {}
        Start-Sleep -Milliseconds 200
    }
    if ($null -eq $entryEvent) { throw "argument hook event missing" }
    if ($null -eq $returnEvent) { throw "return hook event missing" }

    while (-not $pinProcess.HasExited -and [datetime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
    if (-not $pinProcess.HasExited) { throw "target did not exit within $TimeoutSec seconds" }
    [void]$pinProcess.WaitForExit()
    [System.IO.File]::WriteAllText($outPath, $stdoutTask.Result)
    [System.IO.File]::WriteAllText($errPath, $stderrTask.Result)
    $targetOutput = $stdoutTask.Result.Trim()
    if ($targetOutput -notmatch "hooked=4660") { throw "return value was not changed: $targetOutput" }
    $originalArgument = if ($Arch -eq "x86") { [uint64]$entryEvent.arg4 } else { [uint64]$entryEvent.arg0 }
    $originalReturn = [uint64]$returnEvent.arg0
    if ($originalArgument -ne 5) { throw "captured argument was $originalArgument, expected 5" }
    if ($originalReturn -ne 30) { throw "captured return was $originalReturn, expected 30" }
    [ordered]@{
        arch = $Arch
        entry = $entry
        return_address = $returnAddress
        argument_register = $argRegister
        return_register = $returnRegister
        argument_rule_ok = [bool]$argRule.ok
        return_rule_ok = [bool]$returnRule.ok
        original_argument = $originalArgument
        modified_argument = 20
        original_return = $originalReturn
        modified_return = 0x1234
        target_output = $targetOutput
        agent_log = $log
        stdout = $outPath
        stderr = $errPath
    } | ConvertTo-Json -Depth 4
} catch {
    [System.IO.File]::WriteAllText($failurePath, $_.Exception.ToString())
    throw
} finally {
    if ($null -ne $pinProcess -and -not $pinProcess.HasExited) { $pinProcess.Kill(); $pinProcess.WaitForExit() }
    if ($null -eq $oldPort) { Remove-Item Env:PINBRIDGE_AGENT_PORT -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_PORT = $oldPort }
    if ($null -eq $oldLog) { Remove-Item Env:PINBRIDGE_AGENT_LOG -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_LOG = $oldLog }
    if ($null -eq $oldEngines) { Remove-Item Env:PINBRIDGE_AGENT_ENGINES -ErrorAction SilentlyContinue } else { $env:PINBRIDGE_AGENT_ENGINES = $oldEngines }
}
