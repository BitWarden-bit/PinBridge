$repo = Split-Path -Parent $PSScriptRoot
$target = (Get-Content -Encoding UTF8 -Raw (Join-Path $repo 'build\vmp_target.txt')).Trim()
$env:PINBRIDGE_UI_SCRIPT = 'sleep 3500; si; sleep 400; si; sleep 400; si; sleep 400; si; sleep 400; si; sleep 400; watch 4'
Start-Process (Join-Path $repo 'bindings\rust\target\debug\pinbridge-ui.exe') -ArgumentList '--', $target
