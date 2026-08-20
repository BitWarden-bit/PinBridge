[CmdletBinding()]
param(
    [ValidateRange(30, 120)]
    [int]$TimeoutSec = 60,
    [string]$AgentPath = "",
    [string]$CliPath = "",
    [string]$HubPath = "",
    [string]$McpPath = "",
    [switch]$TraceOnly
)

$ErrorActionPreference = "Stop"
$dir = $PSScriptRoot
$repo = (Resolve-Path -LiteralPath (Join-Path $dir "..\..")).Path
$bundle = Split-Path -Parent $repo
$release = Join-Path $repo "bindings\rust\target\release"
$target = Join-Path $dir "hook_python_demo_x64.exe"
$plugin = Join-Path $dir "hook_intercept.py"
$agent = if ($AgentPath) { $AgentPath } else { Join-Path $release "pinbridge_agent.dll" }
$cli = if ($CliPath) { $CliPath } else { Join-Path $release "pinbridge-cli.exe" }
$hubExe = if ($HubPath) { $HubPath } else { Join-Path $release "pinbridge-hub.exe" }
$mcpExe = if ($McpPath) { $McpPath } else { Join-Path $release "pinbridge-mcp.exe" }
$pin = Join-Path $bundle "VMP_Offline_Recovery_Kit_20260803_FINAL\runtime\pin\intel64\bin\pin.exe"

foreach ($path in @($target, $plugin, $agent, $cli, $hubExe, $mcpExe, $pin)) {
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
    if ($length -eq 0 -or $length -gt 16MB) { throw "invalid Hub IPC frame length: $length" }
    [byte[]]$body = Read-Exact -Stream $Stream -Count ([int]$length)
    return ([System.Text.Encoding]::UTF8.GetString($body) | ConvertFrom-Json)
}

function Invoke-HumanHandoff {
    param(
        [Parameter(Mandatory = $true)][int]$Port,
        [Parameter(Mandatory = $true)][string]$Secret,
        [Parameter(Mandatory = $true)][datetime]$Deadline
    )
    while ([datetime]::UtcNow -lt $Deadline) {
        $client = $null
        try {
            $client = [System.Net.Sockets.TcpClient]::new()
            $client.ReceiveTimeout = 15000
            $client.SendTimeout = 15000
            $client.Connect([System.Net.IPAddress]::Loopback, $Port)
            $stream = $client.GetStream()
            Write-Frame -Stream $stream -Value ([ordered]@{
                channel = "human"
                secret = $Secret
            })
            Write-Frame -Stream $stream -Value ([ordered]@{
                id = "human-handoff"
                method = "control_handoff_to_ai"
                params = [ordered]@{
                    mode = "ai_autonomous"
                    purpose = "authorize the isolated MCP callback-Hook regression"
                }
            })
            $response = Read-Frame -Stream $stream
            if (-not $response.ok) { throw "Hub handoff failed: $($response.error)" }
            return $response
        } catch {
            if ($null -ne $client) { $client.Dispose() }
            Start-Sleep -Milliseconds 50
            continue
        } finally {
            if ($null -ne $client) { $client.Dispose() }
        }
    }
    throw "Hub did not accept the trusted-human handoff"
}

$script:McpProcess = $null
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
    $script:McpProcess.StandardInput.WriteLine(
        ($request | ConvertTo-Json -Depth 10 -Compress)
    )
    $script:McpProcess.StandardInput.Flush()
    # The adapter emits exactly one flushed JSON line per request. A direct
    # blocking read avoids PowerShell's synchronization-context interaction
    # with Task.Wait on redirected process streams.
    $line = $script:McpProcess.StandardOutput.ReadLine()
    if (-not $line) { throw "MCP process closed stdout during $Method" }
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
    $response = Invoke-McpRequest -Method "tools/call" -Params ([ordered]@{
        name = $Name
        arguments = $Arguments
    })
    if ($response.result.isError) {
        throw "MCP tool $Name failed: $($response.result.content[0].text)"
    }
    return $response.result.structuredContent
}

$agentPort = Get-FreePort
$hubPort = Get-FreePort
while ($hubPort -eq $agentPort) { $hubPort = Get-FreePort }
$humanSecret = "mcp-human-regression-20260819"
$aiSecret = "mcp-ai-regression-20260819"
$deadline = [datetime]::UtcNow.AddSeconds($TimeoutSec)
$registrationReady = Join-Path $dir "hook_registration.ready"
$agentLog = Join-Path $dir ("hook_mcp_{0}.agent.log" -f $agentPort)
$pinProcess = $null
$hubProcess = $null
$mcpProcess = $null
$pinStdout = $null
$pinStderr = $null
$hubStdout = $null
$hubStderr = $null
$mcpStderr = $null

try {
    Remove-Item -LiteralPath $registrationReady -Force -ErrorAction SilentlyContinue

    $pinStart = [System.Diagnostics.ProcessStartInfo]::new()
    $pinStart.FileName = $pin
    $pinStart.Arguments = '-t "{0}" -- "{1}"' -f $agent, $target
    $pinStart.WorkingDirectory = $dir
    $pinStart.UseShellExecute = $false
    $pinStart.CreateNoWindow = $true
    $pinStart.RedirectStandardOutput = $true
    $pinStart.RedirectStandardError = $true
    $pinStart.Environment["PINBRIDGE_AGENT_PORT"] = $agentPort.ToString()
    $pinStart.Environment["PINBRIDGE_AGENT_LOG"] = $agentLog
    $pinStart.Environment["PINBRIDGE_AGENT_ENGINES"] = "none"
    $pinStart.Environment["PINBRIDGE_SYMBOLS"] = "full"
    $pinProcess = [System.Diagnostics.Process]::new()
    $pinProcess.StartInfo = $pinStart
    [void]$pinProcess.Start()
    $pinStdout = $pinProcess.StandardOutput.ReadToEndAsync()
    $pinStderr = $pinProcess.StandardError.ReadToEndAsync()

    $connected = $false
    while ([datetime]::UtcNow -lt $deadline -and -not $pinProcess.HasExited) {
        try {
            $pong = & $cli --port $agentPort ping 2>$null
            if ($LASTEXITCODE -eq 0 -and $pong) { $connected = $true; break }
        } catch {}
        Start-Sleep -Milliseconds 50
    }
    if (-not $connected) { throw "Agent control plane did not become ready" }

    $hubStart = [System.Diagnostics.ProcessStartInfo]::new()
    $hubStart.FileName = $hubExe
    $hubStart.Arguments = "--agent-port $agentPort --listen $hubPort"
    $hubStart.WorkingDirectory = $release
    $hubStart.UseShellExecute = $false
    $hubStart.CreateNoWindow = $true
    $hubStart.RedirectStandardOutput = $true
    $hubStart.RedirectStandardError = $true
    $hubStart.Environment["PINBRIDGE_HUB_HUMAN_SECRET"] = $humanSecret
    $hubStart.Environment["PINBRIDGE_HUB_AI_SECRET"] = $aiSecret
    $hubProcess = [System.Diagnostics.Process]::new()
    $hubProcess.StartInfo = $hubStart
    [void]$hubProcess.Start()
    $hubStdout = $hubProcess.StandardOutput.ReadToEndAsync()
    $hubStderr = $hubProcess.StandardError.ReadToEndAsync()

    $handoff = Invoke-HumanHandoff -Port $hubPort -Secret $humanSecret -Deadline $deadline
    Write-Host "MCP_TEST_STAGE handoff"

    $mcpStart = [System.Diagnostics.ProcessStartInfo]::new()
    $mcpStart.FileName = $mcpExe
    $mcpStart.Arguments = "--hub-endpoint 127.0.0.1:$hubPort"
    $mcpStart.WorkingDirectory = $release
    $mcpStart.UseShellExecute = $false
    $mcpStart.CreateNoWindow = $true
    $mcpStart.RedirectStandardInput = $true
    $mcpStart.RedirectStandardOutput = $true
    $mcpStart.RedirectStandardError = $true
    $mcpStart.Environment["PINBRIDGE_HUB_AI_SECRET"] = $aiSecret
    $mcpProcess = [System.Diagnostics.Process]::new()
    $mcpProcess.StartInfo = $mcpStart
    [void]$mcpProcess.Start()
    $script:McpProcess = $mcpProcess
    $mcpStderr = $mcpProcess.StandardError.ReadToEndAsync()

    Write-Host "MCP_TEST_STAGE initialize.request"
    $initialize = Invoke-McpRequest -Method "initialize" -Params ([ordered]@{
        protocolVersion = "2025-06-18"
        capabilities = @{}
        clientInfo = [ordered]@{ name = "pinbridge-mcp-hook-regression"; version = "1.0" }
    })
    Write-Host "MCP_TEST_STAGE initialize.ok"
    $tools = Invoke-McpRequest -Method "tools/list" -Params @{}
    Write-Host "MCP_TEST_STAGE tools.ok"
    $toolNames = @($tools.result.tools | ForEach-Object { $_.name })
    $requiredTools = if ($TraceOnly) {
        @(
            "modules_list",
            "trace_scope_query",
            "trace_record_start",
            "trace_record_status",
            "trace_record_stop",
            "trace_index_query",
            "trace_index_export"
        )
    } else {
        @("script_inject", "hook_inventory", "event_index_query")
    }
    foreach ($required in $requiredTools) {
        if ($toolNames -notcontains $required) { throw "MCP tool is missing: $required" }
    }

    Write-Host "MCP_TEST_STAGE control.request"
    $control = Invoke-McpTool -Name "control_status"
    Write-Host "MCP_TEST_STAGE control.ok"
    if ($control.mode -ne "ai_autonomous" -or -not $control.can_ai_write) {
        throw "MCP did not observe ai_autonomous control: $($control | ConvertTo-Json -Compress)"
    }
    $session = Invoke-McpTool -Name "session_status"
    Write-Host "MCP_TEST_STAGE session.ok"
    if ($TraceOnly) {
        $modules = Invoke-McpTool -Name "modules_list"
        $mainModule = @($modules.modules | Where-Object { $_.is_main })[0]
        if ($null -eq $mainModule) { throw "MCP Trace test could not find the main module" }
        $scope = Invoke-McpTool -Name "trace_scope_query" -Arguments @{
            module = [string]$mainModule.name
            kinds = @("exec", "memory", "branch")
            purpose = "freeze the main-module Trace scope before recording"
        }
        if ($scope.next_call.tool -ne "trace_record_start") {
            throw "Trace scope query did not provide an explicit next call"
        }
        $started = Invoke-McpTool -Name "trace_record_start" -Arguments @{
            selection_id = [string]$scope.selection_id
            expected_count = [string]$scope.selected_count
            selection_digest = [string]$scope.selection_digest
            filename = "mcp-live-trace.pbtr"
            purpose = "record a bounded live Trace from the confirmed scope"
        }
        Start-Sleep -Milliseconds 100
        $paused = Invoke-McpTool -Name "target_pause" -Arguments @{
            purpose = "freeze the target before draining the Trace"
        }
        $status = Invoke-McpTool -Name "trace_record_status"
        $stopped = Invoke-McpTool -Name "trace_record_stop" -Arguments @{
            purpose = "drain the completed Trace artifact for indexed reading"
        }
        $indexed = Invoke-McpTool -Name "trace_index_query" -Arguments @{
            index = "kind"
            key = "exec"
            limit = "5"
            fields = @("sequence", "kind", "thread_id", "address", "bytes")
            purpose = "read only five exact execution rows to bound MCP context"
        }
        $exported = Invoke-McpTool -Name "trace_index_export" -Arguments @{
            index = "kind"
            key = "exec"
            limit = "5"
            payload = $true
            format = "jsonl"
            delivery = "file"
            filename = "mcp-live-trace-index"
            purpose = "export the same bounded Trace index without inline payload"
        }
        if ([uint64]$stopped.recorded -eq 0) { throw "MCP Trace recorded zero events" }
        if ([uint64]$indexed.returned -eq 0) { throw "MCP Trace index returned zero rows" }
        if (-not (Test-Path -LiteralPath ([string]$stopped.path) -PathType Leaf)) {
            throw "MCP Trace artifact is missing: $($stopped.path)"
        }
        if (-not (Test-Path -LiteralPath ([string]$exported.path) -PathType Leaf)) {
            throw "MCP Trace export is missing: $($exported.path)"
        }
        [void](Invoke-McpTool -Name "target_resume" -Arguments @{
            purpose = "resume the fixture after Trace validation"
        })
        New-Item -ItemType File -Path $registrationReady -Force | Out-Null
        if (-not $pinProcess.WaitForExit(5000)) { throw "Trace fixture did not exit" }
        [ordered]@{
            result = "MCP_TRACE_PASS"
            protocol = $initialize.result.protocolVersion
            module = $scope.module
            selection_id = $scope.selection_id
            selection_digest = $scope.selection_digest
            selected_ranges = $scope.selected_count
            started_state = $started.state
            status_state = $status.state
            recorded = $stopped.recorded
            dropped = $stopped.dropped
            trace_path = $stopped.path
            query_index = $indexed.index
            query_key = $indexed.key
            query_returned = $indexed.returned
            query_next_before = $indexed.next_before
            export_path = $exported.path
            export_rows = $exported.rows
            target_exit = $pinProcess.ExitCode
        } | ConvertTo-Json -Depth 6
        return
    }
    [string]$source = [System.IO.File]::ReadAllText($plugin)
    $injected = Invoke-McpTool -Name "script_inject" -Arguments @{
        name = "mcp_hook_intercept.py"
        source = $source
        purpose = "verify MCP-created synchronous Hook callbacks and write-back"
    }
    Write-Host "MCP_TEST_STAGE inject.ok"
    $scriptActivityRecord = Invoke-McpTool -Name "activity_get" -Arguments @{
        operation_id = [string]$injected.operation_id
    }
    $scriptActivity = $scriptActivityRecord.activity.actor -eq "ai" -and
        $scriptActivityRecord.activity.action -eq "script_inject" -and
        $scriptActivityRecord.activity.outcome -eq "ok"
    if (-not $scriptActivity) {
        throw "Hub did not attribute successful script_inject activity to AI"
    }

    $captured = @{}
    $maxCallbacks = 0
    $maxHookEvents = 0
    $lastInventory = $null
    $lastMonitor = $null
    while ([datetime]::UtcNow -lt $deadline -and -not $pinProcess.HasExited) {
        try {
            $output = Invoke-McpTool -Name "script_output" -Arguments @{
                cursor = "0"
                limit = "1024"
            }
            foreach ($line in @($output.lines)) { $captured[[string]$line.seq] = $line }
        } catch {}
        try {
            $lastInventory = Invoke-McpTool -Name "hook_inventory" -Arguments @{
                offset = "0"
                limit = "64"
                kind = "all"
            }
            $callbackCount = @(
                $lastInventory.hooks | ForEach-Object { @($_.callbacks) }
            ).Count
            $maxCallbacks = [Math]::Max($maxCallbacks, $callbackCount)
        } catch {}
        try {
            $lastMonitor = Invoke-McpTool -Name "hook_monitor" -Arguments @{ limit = "64" }
            $maxHookEvents = [Math]::Max($maxHookEvents, @($lastMonitor.events).Count)
        } catch {}
        $text = (@($captured.Values | ForEach-Object { $_.line }) -join "`n")
        if ($text.Contains("HOOK_ENTRY_INTERCEPT_PASS") -and
            $text.Contains("HOOK_RETURN_INTERCEPT_PASS") -and
            $maxHookEvents -ge 4) {
            break
        }
        Start-Sleep -Milliseconds 25
    }

    if (-not $pinProcess.HasExited) {
        if (-not $pinProcess.WaitForExit(5000)) { throw "target did not exit" }
    }
    $outputText = (@($captured.Values | Sort-Object { [uint64]$_.seq } |
        ForEach-Object { $_.line }) -join "`n")
    foreach ($marker in @(
        "HOOK_INTERCEPT_READY",
        "HOOK_ENTRY_INTERCEPT_PASS",
        "HOOK_RETURN_INTERCEPT_PASS"
    )) {
        if (-not $outputText.Contains($marker)) { throw "missing MCP callback marker: $marker" }
    }
    if ($pinProcess.ExitCode -ne 0) {
        throw "MCP callback target failed: exit=$($pinProcess.ExitCode) stdout=$($pinStdout.Result.Trim()) stderr=$($pinStderr.Result.Trim())"
    }
    if ($maxCallbacks -lt 1) { throw "MCP Hook inventory never exposed a callback binding" }
    if ($maxHookEvents -lt 4) { throw "MCP Hook monitor saw only $maxHookEvents events" }

    [ordered]@{
        result = "MCP_CALLBACK_HOOK_PASS"
        protocol = $initialize.result.protocolVersion
        control_mode = $control.mode
        ai_write_authorized = [bool]$control.can_ai_write
        session_connected = [bool]$session.session.connected
        script = [ordered]@{
            name = $injected.name
            created_by = $injected.created_by
            operation_id = $injected.operation_id
        }
        callback_bindings_seen = $maxCallbacks
        hook_events_seen = $maxHookEvents
        entry_callback = $outputText.Contains("HOOK_ENTRY_INTERCEPT_PASS")
        return_callback = $outputText.Contains("HOOK_RETURN_INTERCEPT_PASS")
        target_exit = $pinProcess.ExitCode
        target_output = $pinStdout.Result.Trim()
        hub_activity_attributed_to_ai = $scriptActivity
        agent_log = $agentLog
    } | ConvertTo-Json -Depth 6
} finally {
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
    if ($null -ne $pinProcess -and -not $pinProcess.HasExited) {
        $pinProcess.Kill()
        $pinProcess.WaitForExit()
    }
    Remove-Item -LiteralPath $registrationReady -Force -ErrorAction SilentlyContinue
}
