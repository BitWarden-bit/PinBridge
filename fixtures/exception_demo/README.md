# Exception event demo

This fixture installs a vectored exception handler (VEH), then triggers and
handles Windows access violation, breakpoint/INT3, and integer divide-by-zero
exceptions through SEH. It is a runtime test of PinBridge's `context_change`
event, rather than a policy read/write test.

Build and run either architecture from the repository root:

```powershell
.\fixtures\exception_demo\build.ps1 -Arch x64
.\fixtures\exception_demo\run.ps1 -Arch x64
.\fixtures\exception_demo\run.ps1 -Arch x64 -Takeover

.\fixtures\exception_demo\build.ps1 -Arch x86
.\fixtures\exception_demo\run.ps1 -Arch x86
.\fixtures\exception_demo\run.ps1 -Arch x86 -Takeover
```

The runner checks `reason=4` events for `0xC0000005`, `0x80000003`, and
`0xC0000094` (with two access violations), a non-zero exception IP, and a
valid Pin thread id. It also requires the target to report all three SEH
handlers and the VEH observer, then exit normally.

`-Takeover` enables `exc all`, requires the agent to pause at every exception,
resumes each stop through the control plane, and still requires normal target
exception dispatch and exit afterward.
