[CmdletBinding()]
param(
    [ValidateSet("x86", "x64")]
    [string]$Arch = "x64",
    [string]$PinRoot = $env:PIN_ROOT,
    [string]$PinExe = $env:PINBRIDGE_PIN_EXE,
    [ValidateRange(45, 300)]
    [int]$TimeoutSec = 75,
    [switch]$ModifyHook
)

$ErrorActionPreference = "Stop"
$fixtureDir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $fixtureDir "..\..")).Path
$bundle = Split-Path -Parent $repo
$rustTarget = Join-Path $repo "bindings\rust\target"
$cliPath = Join-Path $rustTarget "release\pinbridge-cli.exe"
$fixturePath = Join-Path $fixtureDir ("hook_syscall_{0}.exe" -f $Arch)
$agentPath = if ($Arch -eq "x86") {
    Join-Path $rustTarget "i686-pc-windows-msvc\release\pinbridge_agent.dll"
} else {
    Join-Path $rustTarget "release\pinbridge_agent.dll"
}
$pinArch = if ($Arch -eq "x86") { "ia32" } else { "intel64" }

if (-not $PinExe) {
    if (-not $PinRoot) {
        $candidate = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin"
        if (Test-Path -LiteralPath $candidate) {
            $PinRoot = $candidate
        }
    }
    if ($PinRoot) {
        $PinExe = Join-Path $PinRoot ("{0}\bin\pin.exe" -f $pinArch)
    }
}

foreach ($required in @($cliPath, $fixturePath, $agentPath, $PinExe)) {
    if (-not $required -or -not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "required file not found: $required"
    }
}

$ntdllScript = Join-Path $repo "examples\python\ntdll_trace.py"
$syscallScript = Join-Path $repo "examples\python\syscall_watch.py"
$modifyScript = Join-Path $fixtureDir "hook_modify.py"
$probeScripts = @($ntdllScript, $syscallScript)
if ($ModifyHook) { $probeScripts = @($modifyScript) + $probeScripts }
foreach ($scriptPath in $probeScripts) {
    if (-not (Test-Path -LiteralPath $scriptPath -PathType Leaf)) {
        throw "probe script not found: $scriptPath"
    }
}

function Get-FreeTcpPort {
    $listener = [System.Net.Sockets.TcpListener]::new(
        [System.Net.IPAddress]::Loopback, 0)
    try {
        $listener.Start()
        return ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
    } finally {
        $listener.Stop()
    }
}

$port = Get-FreeTcpPort
$tag = "{0}_{1}" -f $Arch, $port
$agentLog = Join-Path $fixtureDir ("run_{0}.agent.log" -f $tag)
$targetOut = Join-Path $fixtureDir ("run_{0}.stdout.txt" -f $tag)
$targetErr = Join-Path $fixtureDir ("run_{0}.stderr.txt" -f $tag)
$script:CliPath = $cliPath
$script:Port = $port

function Invoke-CliRaw {
    param([Parameter(Mandatory = $true)][string[]]$Command)

    # Windows PowerShell 5.1 promotes redirected native stderr to an
    # ErrorRecord when ErrorActionPreference=Stop. Use Process directly so
    # expected connect failures remain retryable during startup.
    $allArguments = @("--port", $script:Port.ToString()) + $Command
    $quotedArguments = @($allArguments | ForEach-Object {
        '"' + ([string]$_).Replace('"', '\"') + '"'
    })
    $startInfo = [System.Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $script:CliPath
    $startInfo.Arguments = $quotedArguments -join " "
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = [System.Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    [void]$process.Start()
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(15000)) {
        $process.Kill()
        $process.WaitForExit()
        return [pscustomobject]@{
            Ok = $false
            ExitCode = -1
            Text = "pinbridge-cli timed out after 15 seconds"
        }
    }
    $stdout = $stdoutTask.Result.Trim()
    $stderr = $stderrTask.Result.Trim()
    $exitCode = $process.ExitCode
    $text = if ($exitCode -eq 0) { $stdout } else {
        (@($stderr, $stdout) | Where-Object { $_ }) -join "`n"
    }
    return [pscustomobject]@{
        Ok = ($exitCode -eq 0)
        ExitCode = $exitCode
        Text = $text
    }
}

function Invoke-CliJson {
    param([Parameter(Mandatory = $true)][string[]]$Command)

    $reply = Invoke-CliRaw -Command $Command
    if (-not $reply.Ok) {
        throw "pinbridge-cli $($Command -join ' ') failed: $($reply.Text)"
    }
    try {
        return ($reply.Text | ConvertFrom-Json)
    } catch {
        throw "invalid JSON from pinbridge-cli $($Command -join ' '): $($reply.Text)"
    }
}

function Test-PluginLoaded {
    param([Parameter(Mandatory = $true)][string]$Name)

    $reply = Invoke-CliRaw -Command @("script", "list")
    if (-not $reply.Ok) { return $false }
    try {
        $plugins = @(($reply.Text | ConvertFrom-Json).plugins)
        return [bool]($plugins | Where-Object { $_.name -eq $Name })
    } catch {
        return $false
    }
}

function Invoke-ScriptLoad {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][datetime]$Deadline
    )

    $name = Split-Path -Leaf $Path
    $lastError = ""
    while ([datetime]::UtcNow -lt $Deadline) {
        $reply = Invoke-CliRaw -Command @("script", "run", $Path)
        if ($reply.Ok) {
            return ($reply.Text | ConvertFrom-Json)
        }
        $lastError = $reply.Text

        # SCRIPT_LOAD replies after compile, while the published list is
        # updated on a following host tick. Give that asynchronous publish a
        # bounded settle window before retrying, otherwise a slow load can be
        # submitted twice and replace the plugin that just started.
        $settleDeadline = [datetime]::UtcNow.AddSeconds(3)
        while ([datetime]::UtcNow -lt $settleDeadline) {
            if (Test-PluginLoaded -Name $name) {
                return [pscustomobject]@{ name = $name; recovered_after_timeout = $true }
            }
            Start-Sleep -Milliseconds 200
        }
        Start-Sleep -Milliseconds 400
    }
    throw "timed out loading $name`: $lastError"
}

function Wait-ForOutputLine {
    param(
        [Parameter(Mandatory = $true)][string]$Pattern,
        [Parameter(Mandatory = $true)][datetime]$Deadline
    )

    while ([datetime]::UtcNow -lt $Deadline) {
        $reply = Invoke-CliRaw -Command @("script", "output")
        if ($reply.Ok) {
            try {
                $lines = @((($reply.Text | ConvertFrom-Json).lines))
                if ((($lines | ForEach-Object { $_.line }) -join "`n") -match $Pattern) {
                    return $lines
                }
            } catch {
                # The next poll will retry while the script host is busy.
            }
        }
        Start-Sleep -Milliseconds 200
    }
    throw "timed out waiting for script output: $Pattern"
}

$oldPort = $env:PINBRIDGE_AGENT_PORT
$oldLog = $env:PINBRIDGE_AGENT_LOG
$oldEngines = $env:PINBRIDGE_AGENT_ENGINES
$oldPythonHome = $env:PYTHONHOME
$pinProcess = $null
$pinStdoutTask = $null
$pinStderrTask = $null
$targetPid = $null
$targetName = [System.IO.Path]::GetFileNameWithoutExtension($fixturePath)

try {
    # The release directory always carries python310.dll, but a developer
    # build may rely on an installed 3.10 stdlib instead of python310.zip.
    # Locate that home for the child when the caller did not set one.
    if ($Arch -eq "x64" -and -not $env:PYTHONHOME -and
        -not (Test-Path -LiteralPath (Join-Path (Split-Path $agentPath) "python310.zip"))) {
        $pythonHomes = @()
        $pythonCommand = Get-Command python.exe -CommandType Application `
            -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($null -ne $pythonCommand) {
            $pythonHomes += Split-Path -Parent $pythonCommand.Source
        }
        if ($env:LOCALAPPDATA) {
            $pythonHomes += Join-Path $env:LOCALAPPDATA "Programs\Python\Python310"
        }
        $detectedPythonHome = $pythonHomes | Where-Object {
            (Test-Path -LiteralPath (Join-Path $_ "python310.dll") -PathType Leaf) -and
            (Test-Path -LiteralPath (Join-Path $_ "Lib\encodings") -PathType Container)
        } | Select-Object -First 1
        if ($detectedPythonHome) {
            $env:PYTHONHOME = $detectedPythonHome
        }
    }
    $env:PINBRIDGE_AGENT_PORT = $port.ToString()
    $env:PINBRIDGE_AGENT_LOG = $agentLog
    # Avoid filling the ring during Pin image startup. Hook and syscall
    # engines are enabled by the probes after they are loaded.
    $env:PINBRIDGE_AGENT_ENGINES = "syscall"
    $arguments = @("-t", ('"' + $agentPath + '"'), "--", ('"' + $fixturePath + '"'))
    # Start-Process can reject a Windows environment containing both Path and
    # PATH (case-insensitive duplicate). ProcessStartInfo inherits the block
    # directly and avoids that PowerShell 5.1 conversion failure.
    $pinStart = [System.Diagnostics.ProcessStartInfo]::new()
    $pinStart.FileName = $PinExe
    $pinStart.Arguments = $arguments -join " "
    $pinStart.WorkingDirectory = $fixtureDir
    $pinStart.UseShellExecute = $false
    $pinStart.CreateNoWindow = $true
    $pinStart.RedirectStandardOutput = $true
    $pinStart.RedirectStandardError = $true
    $pinProcess = [System.Diagnostics.Process]::new()
    $pinProcess.StartInfo = $pinStart
    [void]$pinProcess.Start()
    $pinStdoutTask = $pinProcess.StandardOutput.ReadToEndAsync()
    $pinStderrTask = $pinProcess.StandardError.ReadToEndAsync()

    $readyDeadline = [datetime]::UtcNow.AddSeconds(30)
    $ping = $null
    while ([datetime]::UtcNow -lt $readyDeadline) {
        if ($pinProcess.HasExited) {
            throw "Pin exited before the agent became ready (exit=$($pinProcess.ExitCode))"
        }
        $reply = Invoke-CliRaw -Command @("ping")
        if ($reply.Ok) {
            try { $ping = $reply.Text | ConvertFrom-Json } catch { $ping = $null }
        }
        $pythonReady = (Test-Path -LiteralPath $agentLog) -and
            [bool](Select-String -LiteralPath $agentLog `
                -SimpleMatch "python interpreter initialized" -Quiet)
        $pythonDisabled = (Test-Path -LiteralPath $agentLog) -and
            [bool](Select-String -LiteralPath $agentLog `
                -SimpleMatch "python scripting disabled in this build" -Quiet)
        if ($pythonDisabled) {
            throw "Python scripting is disabled in the $Arch agent build"
        }
        if ($null -ne $ping -and $pythonReady) { break }
        Start-Sleep -Milliseconds 250
    }
    if ($null -eq $ping) { throw "agent did not answer ping within 30 seconds" }
    if (-not $pythonReady) {
        throw "Python scripting did not initialize; this agent may be a no-scripting build"
    }
    $targetPid = [int]$ping.pid

    # These probes consume hook/syscall records only. Disable the default
    # memory/exec/branch flood so their cursors do not overrun the ring before
    # the fixture reaches its delayed Nt* calls.
    foreach ($engine in @(2, 3, 4)) {
        [void](Invoke-CliJson -Command @("engine", $engine.ToString(), "off"))
    }

    # Load probes strictly in order. The load helper handles slow pb_init
    # without racing a duplicate request against the first one.
    $loadDeadline = [datetime]::UtcNow.AddSeconds(30)
    $ntdllLoad = Invoke-ScriptLoad -Path $ntdllScript -Deadline $loadDeadline
    # pb_init() arms thousands of addresses through the single-threaded
    # query server. Do not submit the second SCRIPT_LOAD until that batch has
    # completed, or its mailbox request can interrupt the batch and leave a
    # partially armed set.
    $armLines = Wait-ForOutputLine -Pattern "unique hooks armed" `
        -Deadline ([datetime]::UtcNow.AddSeconds(20))
    $modifyLoad = $null
    if ($ModifyHook) {
        $modifyLoad = Invoke-ScriptLoad -Path $modifyScript -Deadline $loadDeadline
        [void](Wait-ForOutputLine -Pattern "hook_modify: armed" `
            -Deadline ([datetime]::UtcNow.AddSeconds(20)))
    }
    $syscallLoad = Invoke-ScriptLoad -Path $syscallScript -Deadline $loadDeadline

    $exports = Invoke-CliJson -Command @("exports", "ntdll.dll")
    $hooks = Invoke-CliJson -Command @("hooks")
    $scripts = Invoke-CliJson -Command @("script", "list")

    # The fixture calls its Nt* functions after 30 seconds and then stays up
    # for another 10 seconds. Poll during that window because RPC disappears
    # as soon as the target exits.
    $deadline = [datetime]::UtcNow.AddSeconds($TimeoutSec)
    $lastOutput = $null
    $lastCounters = $null
    $sawNtdllHit = $false
    $sawSyscall = $false
    $sawModify = $false
    while ([datetime]::UtcNow -lt $deadline) {
        $target = Get-Process -Id $targetPid -ErrorAction SilentlyContinue
        if ($null -eq $target) { break }

        $outputReply = Invoke-CliRaw -Command @("script", "output")
        if ($outputReply.Ok) {
            try { $lastOutput = $outputReply.Text | ConvertFrom-Json } catch {}
        }
        $counterReply = Invoke-CliRaw -Command @("counters")
        if ($counterReply.Ok) {
            try { $lastCounters = $counterReply.Text | ConvertFrom-Json } catch {}
        }
        if ($null -ne $lastOutput) {
            $text = (@($lastOutput.lines) | ForEach-Object { $_.line }) -join "`n"
            $sawNtdllHit = $text.Contains("NTDLL_HIT ")
            $sawSyscall = $text.Contains("SYSCALL ")
            $sawModify = $text.Contains("HOOK_ACTION ")
        }
        Start-Sleep -Milliseconds $(if ($sawNtdllHit -and $sawSyscall) { 500 } else { 250 })
    }

    if (Get-Process -Id $targetPid -ErrorAction SilentlyContinue) {
        throw "fixture did not exit within $TimeoutSec seconds"
    }
    if (-not $sawNtdllHit) {
        throw "fixture completed without an NTDLL hook hit"
    }
    if (-not $sawSyscall) {
        throw "fixture completed without a syscall event"
    }
    if ($ModifyHook -and -not $sawModify) {
        throw "fixture completed without a synchronous Hook action"
    }
    [void]$pinProcess.WaitForExit(10000)
    if ($null -ne $pinStdoutTask) {
        [System.IO.File]::WriteAllText($targetOut, $pinStdoutTask.Result)
    }
    if ($null -ne $pinStderrTask) {
        [System.IO.File]::WriteAllText($targetErr, $pinStderrTask.Result)
    }

    $outputLines = if ($null -ne $lastOutput) { @($lastOutput.lines) } else { @() }
    $interesting = @($outputLines | Where-Object {
        $_.line -match "^(hook_modify:|HOOK_ACTION |ntdll_trace:|NTDLL_HIT |SYSCALL |syscall_watch:)"
    } | Select-Object -First 16 | ForEach-Object { $_.line })
    $stdout = if (Test-Path -LiteralPath $targetOut) {
        (Get-Content -LiteralPath $targetOut -Raw).Trim()
    } else { "" }

    $summary = [ordered]@{
        arch = $Arch
        port = $port
        target_pid = $targetPid
        target_exited = $true
        launcher_exit_code = if ($pinProcess.HasExited) { $pinProcess.ExitCode } else { $null }
        exports = [ordered]@{ module = "ntdll.dll"; count = [int]$exports.count }
        hooks = [ordered]@{ count = [int]$hooks.count }
        scripts = @($scripts.plugins | ForEach-Object { $_.name })
        observations = [ordered]@{
            ntdll_hit = $sawNtdllHit
            syscall = $sawSyscall
            hook_action = $sawModify
            output_lines = $outputLines.Count
            sample = $interesting
        }
        counters = $lastCounters
        target_output = $stdout
        artifacts = [ordered]@{
            agent_log = $agentLog
            stdout = $targetOut
            stderr = $targetErr
        }
    }
    $summary | ConvertTo-Json -Depth 6
} finally {
    if ($targetPid) {
        $target = Get-Process -Id $targetPid -ErrorAction SilentlyContinue
        if ($null -ne $target -and $target.ProcessName -eq $targetName) {
            Stop-Process -Id $targetPid -Force -ErrorAction SilentlyContinue
        }
    }
    if ($null -ne $pinProcess -and -not $pinProcess.HasExited -and
        $pinProcess.ProcessName -eq "pin") {
        Stop-Process -Id $pinProcess.Id -Force -ErrorAction SilentlyContinue
    }
    if ($null -eq $oldPort) {
        Remove-Item Env:PINBRIDGE_AGENT_PORT -ErrorAction SilentlyContinue
    } else {
        $env:PINBRIDGE_AGENT_PORT = $oldPort
    }
    if ($null -eq $oldLog) {
        Remove-Item Env:PINBRIDGE_AGENT_LOG -ErrorAction SilentlyContinue
    } else {
        $env:PINBRIDGE_AGENT_LOG = $oldLog
    }
    if ($null -eq $oldEngines) {
        Remove-Item Env:PINBRIDGE_AGENT_ENGINES -ErrorAction SilentlyContinue
    } else {
        $env:PINBRIDGE_AGENT_ENGINES = $oldEngines
    }
    if ($null -eq $oldPythonHome) {
        Remove-Item Env:PYTHONHOME -ErrorAction SilentlyContinue
    } else {
        $env:PYTHONHOME = $oldPythonHome
    }
}
