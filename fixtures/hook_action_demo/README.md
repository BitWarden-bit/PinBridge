# Hook action demo

This fixture calls an exported `DemoApi(5)` twice. The first call is a
baseline (`15`). During the 30-second pause, `run.ps1` arms the entry and
`ret` instructions through the control plane. The native action rules then:

- capture the original argument `5` and change it to `20`;
- capture the original return `30` in a `hook_return` event and change RAX/EAX
  to `0x1234`.

Build and run the x64 or x86 version with the same Pin kit used by the other
fixtures:

```powershell
.\build.ps1 -Arch x64
.\run.ps1 -Arch x64
```

`hook_action_demo.py` contains the equivalent in-agent Python policy for the
x64 scripting build. The runner uses the control plane so the same test also
works with the no-CPython x86 agent.

The expected target line is:

```text
hook_action_demo: input=5 baseline=15 hooked=4660
```
