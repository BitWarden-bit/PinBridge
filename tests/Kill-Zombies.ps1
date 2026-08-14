$procs = Get-CimInstance Win32_Process | Where-Object {
    ($_.Name -eq 'cmd.exe' -and $_.CommandLine -match 'for /l') -or
    ($_.Name -eq 'PING.EXE' -and $_.CommandLine -match '-t') -or
    ($_.Name -eq 'pb_rpc_fixture.exe') -or
    ($_.Name -eq 'pb_runtime_fixture.exe')
}
foreach ($p in $procs) {
    $cmd = if ($p.CommandLine) { $p.CommandLine.Substring(0, [Math]::Min(70, $p.CommandLine.Length)) } else { '' }
    Write-Output ("kill {0} {1} {2}" -f $p.ProcessId, $p.Name, $cmd)
    Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue
}
if (-not $procs) { Write-Output "no zombies" }
