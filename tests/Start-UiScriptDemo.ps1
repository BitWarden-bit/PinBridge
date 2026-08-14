$repo = Split-Path -Parent $PSScriptRoot
$env:PINBRIDGE_UI_SCRIPT = 'sleep 2500; bp main+0x1180; resume; watch 10'
Start-Process (Join-Path $repo 'bindings\rust\target\debug\pinbridge-ui.exe') -ArgumentList '--', (Join-Path $repo 'build\host-tests\pb_rpc_fixture.exe')
