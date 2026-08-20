[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [ValidateRange(1, 65535)]
    [int]$AgentPort,
    [string]$ScriptPath = "",
    [string]$ScriptName = "codex_live_strategy.py",
    [string]$InspectScriptName = "",
    [switch]$ResumeTarget,
    [switch]$ProbeCurrentInstruction,
    [ValidateRange(0, 30)]
    [int]$ObserveSeconds = 2,
    [string]$HubPath = "",
    [string]$McpPath = ""
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")).Path
$release = Join-Path $repo "bindings\rust\target\release"
$hubExe = if ($HubPath) { $HubPath } else { Join-Path $release "pinbridge-hub.exe" }
$mcpExe = if ($McpPath) { $McpPath } else { Join-Path $release "pinbridge-mcp.exe" }

foreach ($path in @($hubExe, $mcpExe)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "required file not found: $path"
    }
}
if ($ScriptPath) {
    $ScriptPath = (Resolve-Path -LiteralPath $ScriptPath).Path
}
if ($ProbeCurrentInstruction -and (-not $ScriptPath -or -not $ResumeTarget)) {
    throw "ProbeCurrentInstruction requires both ScriptPath and ResumeTarget"
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

function Write-Frame {
    param(
        [Parameter(Mandatory = $true)][System.IO.Stream]$Stream,
        [Parameter(Mandatory = $true)]$Value
    )
    [byte[]]$body = [System.Text.Encoding]::UTF8.GetBytes(
        ($Value | ConvertTo-Json -Depth 10 -Compress)
    )
    [byte[]]$length = [System.BitConverter]::GetBytes([uint32]$body.Length)
    $Stream.Write($length, 0, $length.Length)
    $Stream.Write($body, 0, $body.Length)
    $Stream.Flush()
}

function Read-Exact {
    param(
        [Parameter(Mandatory = $true)][System.IO.Stream]$Stream,
        [Parameter(Mandatory = $true)][int]$Count
    )
    [byte[]]$buffer = [byte[]]::new($Count)
    $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.Read($buffer, $offset, $Count - $offset)
        if ($read -le 0) { throw "Hub IPC closed before a complete frame" }
        $offset += $read
    }
    return $buffer
}

function Read-Frame {
    param([Parameter(Mandatory = $true)][System.IO.Stream]$Stream)
    [byte[]]$lengthBytes = Read-Exact -Stream $Stream -Count 4
    $length = [System.BitConverter]::ToUInt32($lengthBytes, 0)
    if ($length -eq 0 -or $length -gt 16MB) {
        throw "invalid Hub IPC frame length: $length"
    }
    [byte[]]$body = Read-Exact -Stream $Stream -Count ([int]$length)
    return ([System.Text.Encoding]::UTF8.GetString($body) | ConvertFrom-Json)
}

function Invoke-HumanCall {
    param(
        [Parameter(Mandatory = $true)][int]$Port,
        [Parameter(Mandatory = $true)][string]$Secret,
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params
    )
    $client = [System.Net.Sockets.TcpClient]::new()
    try {
        $client.ReceiveTimeout = 15000
        $client.SendTimeout = 15000
        $client.Connect([System.Net.IPAddress]::Loopback, $Port)
        $stream = $client.GetStream()
        Write-Frame -Stream $stream -Value ([ordered]@{
            channel = "human"
            secret = $Secret
        })
        Write-Frame -Stream $stream -Value ([ordered]@{
            id = "live-human-call"
            method = $Method
            params = $Params
        })
        $response = Read-Frame -Stream $stream
        if (-not $response.ok) { throw "Hub human call failed: $($response.error)" }
        return $response.result
    } finally {
        $client.Dispose()
    }
}

$script:McpProcess = $null
$script:McpStderr = $null
$script:McpRequestId = 0
function Invoke-McpRequest {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)]$Params
    )
    $script:McpRequestId += 1
    $request = [ordered]@{
        jsonrpc = "2.0"
        id = $script:McpRequestId
        method = $Method
        params = $Params
    }
    $jsonLine = ($request | ConvertTo-Json -Depth 10 -Compress) + "`n"
    [byte[]]$requestBytes = [System.Text.Encoding]::UTF8.GetBytes($jsonLine)
    $inputStream = $script:McpProcess.StandardInput.BaseStream
    $inputStream.Write($requestBytes, 0, $requestBytes.Length)
    $inputStream.Flush()
    $line = $script:McpProcess.StandardOutput.ReadLine()
    if (-not $line) {
        $detail = ""
        if ($script:McpProcess.HasExited -and $null -ne $script:McpStderr) {
            $detail = $script:McpStderr.Result.Trim()
        }
        throw "MCP process closed stdout during $Method`: $detail"
    }
    $response = $line | ConvertFrom-Json
    if ($null -ne $response.error) {
        throw "MCP protocol error for $Method`: $($response.error.message)"
    }
    return $response
}

function Invoke-McpTool {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [hashtable]$Arguments = @{}
    )
    Write-Host "MCP_LIVE_STAGE tool=$Name"
    $response = Invoke-McpRequest -Method "tools/call" -Params ([ordered]@{
        name = $Name
        arguments = $Arguments
    })
    if ($response.result.isError) {
        throw "MCP tool $Name failed: $($response.result.content[0].text)"
    }
    return $response.result.structuredContent
}

function Convert-AddressString {
    param([Parameter(Mandatory = $true)][string]$Value)
    if ($Value.StartsWith("0x", [System.StringComparison]::OrdinalIgnoreCase)) {
        return [System.Convert]::ToUInt64($Value.Substring(2), 16)
    }
    return [System.Convert]::ToUInt64($Value, 10)
}

$hubPort = Get-FreePort
$humanSecret = "live-human-" + [Guid]::NewGuid().ToString("N")
$aiSecret = "live-ai-" + [Guid]::NewGuid().ToString("N")
$utf8 = [System.Text.UTF8Encoding]::new($false)
$hubProcess = $null
$mcpProcess = $null
$hubStdout = $null
$hubStderr = $null
$mcpStderr = $null
$targetPausedForProbe = $false
$targetResumeAttempted = $false

try {
    $hubStart = [System.Diagnostics.ProcessStartInfo]::new()
    $hubStart.FileName = $hubExe
    $hubStart.Arguments = "--agent-port $AgentPort --listen $hubPort"
    $hubStart.WorkingDirectory = $release
    $hubStart.UseShellExecute = $false
    $hubStart.CreateNoWindow = $true
    $hubStart.RedirectStandardOutput = $true
    $hubStart.RedirectStandardError = $true
    $hubStart.StandardOutputEncoding = $utf8
    $hubStart.StandardErrorEncoding = $utf8
    $hubStart.Environment["PINBRIDGE_HUB_HUMAN_SECRET"] = $humanSecret
    $hubStart.Environment["PINBRIDGE_HUB_AI_SECRET"] = $aiSecret
    $hubProcess = [System.Diagnostics.Process]::new()
    $hubProcess.StartInfo = $hubStart
    [void]$hubProcess.Start()
    $hubStdout = $hubProcess.StandardOutput.ReadToEndAsync()
    $hubStderr = $hubProcess.StandardError.ReadToEndAsync()

    $handoff = $null
    $deadline = [datetime]::UtcNow.AddSeconds(15)
    while ([datetime]::UtcNow -lt $deadline -and $null -eq $handoff) {
        try {
            $handoff = Invoke-HumanCall -Port $hubPort -Secret $humanSecret `
                -Method "control_handoff_to_ai" -Params ([ordered]@{
                    mode = "ai_autonomous"
                    purpose = "authorize one isolated live MCP inspection/injection"
                })
        } catch {
            if ($hubProcess.HasExited) { throw }
            Start-Sleep -Milliseconds 50
        }
    }
    if ($null -eq $handoff) { throw "Hub did not accept the trusted-human handoff" }

    $mcpStart = [System.Diagnostics.ProcessStartInfo]::new()
    $mcpStart.FileName = $mcpExe
    $mcpStart.Arguments = "--hub-endpoint 127.0.0.1:$hubPort"
    $mcpStart.WorkingDirectory = $release
    $mcpStart.UseShellExecute = $false
    $mcpStart.CreateNoWindow = $true
    $mcpStart.RedirectStandardInput = $true
    $mcpStart.RedirectStandardOutput = $true
    $mcpStart.RedirectStandardError = $true
    $mcpStart.StandardOutputEncoding = $utf8
    $mcpStart.StandardErrorEncoding = $utf8
    $mcpStart.Environment["PINBRIDGE_HUB_AI_SECRET"] = $aiSecret
    $mcpProcess = [System.Diagnostics.Process]::new()
    $mcpProcess.StartInfo = $mcpStart
    [void]$mcpProcess.Start()
    $script:McpProcess = $mcpProcess
    $mcpStderr = $mcpProcess.StandardError.ReadToEndAsync()
    $script:McpStderr = $mcpStderr

    $initialize = Invoke-McpRequest -Method "initialize" -Params ([ordered]@{
        protocolVersion = "2025-06-18"
        capabilities = @{}
        clientInfo = [ordered]@{ name = "pinbridge-live-runner"; version = "1.0" }
    })
    $control = Invoke-McpTool -Name "control_status"
    $session = Invoke-McpTool -Name "session_status"
    $modules = Invoke-McpTool -Name "modules_list"
    $mainModule = @($modules.modules | Where-Object { $_.is_main }) | Select-Object -First 1
    $exports = $null
    $exportsError = $null
    if ($null -ne $mainModule) {
        try {
            $exports = Invoke-McpTool -Name "module_exports" -Arguments @{
                module = [string]$mainModule.name
            }
        } catch {
            $exportsError = $_.Exception.Message
        }
    }

    $probeContext = $null
    if ($ProbeCurrentInstruction) {
        $pauseResult = Invoke-McpTool -Name "target_pause" -Arguments @{
            purpose = "capture one stable live instruction for an exact once callback probe"
        }
        $targetPausedForProbe = [bool]$pauseResult.paused
        $threads = Invoke-McpTool -Name "threads_list"
        $contexts = @()
        foreach ($threadId in @($threads.threads)) {
            try {
                $context = Invoke-McpTool -Name "registers_get" -Arguments @{
                    thread_id = [string]$threadId
                }
                $ripRow = @($context.registers | Where-Object { $_.id -eq "26" }) |
                    Select-Object -First 1
                if ($null -ne $ripRow) {
                    $contexts += [pscustomobject]@{
                        thread_id = [string]$threadId
                        address = [string]$ripRow.value
                        address_value = Convert-AddressString ([string]$ripRow.value)
                    }
                }
            } catch {}
        }
        if ($contexts.Count -eq 0) {
            throw "target pause produced no readable x64 instruction contexts"
        }
        $mainLow = Convert-AddressString ([string]$mainModule.base)
        $mainHigh = Convert-AddressString ([string]$mainModule.end)
        $probeContext = @($contexts | Where-Object {
            $_.address_value -ge $mainLow -and $_.address_value -lt $mainHigh
        }) | Select-Object -First 1
        if ($null -eq $probeContext) { $probeContext = $contexts[0] }
        $stoppedAddress = $probeContext.address
        $decoded = Invoke-McpTool -Name "disassemble" -Arguments @{
            address = $stoppedAddress
            count = "4"
        }
        $instructions = @($decoded.instructions)
        if ($instructions.Count -ge 2) {
            $firstText = [string]$instructions[0].text
            $successor = [string]$instructions[1].address
            $directControlFlow = $firstText.TrimStart().StartsWith(
                "jmp ", [System.StringComparison]::OrdinalIgnoreCase
            ) -or $firstText.TrimStart().StartsWith(
                "call ", [System.StringComparison]::OrdinalIgnoreCase
            )
            if ($directControlFlow -and [string]$instructions[0].target -ne "0x0") {
                $successor = [string]$instructions[0].target
            }
            $probeContext = [pscustomobject]@{
                thread_id = $probeContext.thread_id
                stopped_address = $stoppedAddress
                address = $successor
                address_value = Convert-AddressString $successor
                first_instruction = $firstText
            }
        }
    }

    $scriptResult = $null
    $activity = $null
    $resumeResult = $null
    if ($ScriptPath) {
        [string]$source = [System.IO.File]::ReadAllText($ScriptPath)
        if ($null -ne $probeContext) {
            $source = $source.Replace(
                "LIVE_PROBE_ADDRESS = 0",
                "LIVE_PROBE_ADDRESS = $($probeContext.address)"
            ).Replace(
                "LIVE_PROBE_THREAD_ID = None",
                "LIVE_PROBE_THREAD_ID = $($probeContext.thread_id)"
            )
        }
        $scripts = Invoke-McpTool -Name "script_list"
        $exists = @($scripts.scripts | Where-Object { $_.name -eq $ScriptName }).Count -gt 0
        $tool = if ($exists) { "script_replace" } else { "script_inject" }
        $scriptResult = Invoke-McpTool -Name $tool -Arguments @{
            name = $ScriptName
            source = $source
            purpose = "install a bounded observation strategy for the operator's live sample"
        }
        $activity = Invoke-McpTool -Name "activity_get" -Arguments @{
            operation_id = [string]$scriptResult.operation_id
        }
        if ($ResumeTarget) {
            $targetResumeAttempted = $true
            $resumeResult = Invoke-McpTool -Name "target_resume" -Arguments @{
                purpose = "run the live sample after its callback strategy is armed"
            }
        }
        if ($ObserveSeconds -gt 0) { Start-Sleep -Seconds $ObserveSeconds }
    }

    $statusScriptName = if ($ScriptPath) { $ScriptName } else { $InspectScriptName }
    $scriptStatus = if ($statusScriptName) {
        Invoke-McpTool -Name "script_status" -Arguments @{ name = $statusScriptName }
    } else { $null }
    $scriptOutput = if ($statusScriptName) {
        Invoke-McpTool -Name "script_output" -Arguments @{ cursor = "0"; limit = "512" }
    } else { $null }
    $inventory = Invoke-McpTool -Name "hook_inventory" -Arguments @{
        offset = "0"; limit = "256"; kind = "all"
    }
    $monitor = Invoke-McpTool -Name "hook_monitor" -Arguments @{ limit = "256" }

    [ordered]@{
        result = "MCP_LIVE_SESSION_OK"
        protocol = $initialize.result.protocolVersion
        control_mode = $control.mode
        session = $session.session
        main_module = $mainModule
        main_exports = if ($null -ne $exports) { $exports.exports } else { @() }
        main_exports_error = $exportsError
        live_probe = $probeContext
        script = $scriptResult
        script_activity = if ($null -ne $activity) { $activity.activity } else { $null }
        resume = $resumeResult
        script_status = $scriptStatus
        script_output = if ($null -ne $scriptOutput) { $scriptOutput.lines } else { @() }
        hooks = $inventory.hooks
        hook_events = $monitor.events
    } | ConvertTo-Json -Depth 12
} finally {
    if ($targetPausedForProbe -and -not $targetResumeAttempted -and
        $null -ne $mcpProcess -and -not $mcpProcess.HasExited) {
        try {
            $targetResumeAttempted = $true
            [void](Invoke-McpTool -Name "target_resume" -Arguments @{
                purpose = "failsafe resume after a live callback probe error"
            })
        } catch {}
    }
    if ($null -ne $mcpProcess) {
        try { $mcpProcess.StandardInput.Close() } catch {}
        if (-not $mcpProcess.HasExited -and -not $mcpProcess.WaitForExit(2000)) {
            $mcpProcess.Kill()
            $mcpProcess.WaitForExit()
        }
    }
    if ($null -ne $hubProcess -and -not $hubProcess.HasExited) {
        $hubProcess.Kill()
        $hubProcess.WaitForExit()
    }
}
