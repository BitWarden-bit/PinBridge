# Hook/syscall fixture

`hook_syscall.c` is an in-repo Windows target that resolves and calls three
NTDLL exports (`NtYieldExecution`, `NtQueryInformationProcess`, and `NtClose`),
also exercises `Sleep`, `CreateEventA`, and process/thread queries, and waits
long enough for a controller to arm runtime hooks.

Build either architecture:

```powershell
.\fixtures\hook_syscall\build.ps1 -Arch x64
.\fixtures\hook_syscall\build.ps1 -Arch x86
```

Run the x64 end-to-end Python probe test:

```powershell
.\fixtures\hook_syscall\run_test.ps1 -Arch x64
```

The current x86 agent is built without embedded CPython. Its equivalent
module-wide hook setup is available directly through the control plane:

```powershell
pinbridge-cli --port <port> hookall ntdll.dll
pinbridge-cli --port <port> syscallfilter all
pinbridge-cli --port <port> counters
```

`hookall` enumerates PE32 or PE32+ exports, removes address aliases such as
`Nt*`/`Zw*`, and arms every unique address up to the 4096-point agent limit.

## Synchronous parameter modification

`hook_modify.py` demonstrates a real write-back action. It arms `NtClose`,
installs `pb.hook_rule(ntclose, "rcx", 0)`, and the native Hook callback
changes the live RCX before `NtClose` runs. The target prints a failing
`close=0xc0000008` instead of closing the valid event handle. The event line
still reports the original argument, so observation and modification are both
visible. Run the combined x64 probe with:

```powershell
.\fixtures\hook_syscall\run_test.ps1 -Arch x64 -ModifyHook
```

The no-Python x86 control-plane runner uses the same action. Because x86
`NtClose` is stdcall, its first argument is selected as the ABI-aware virtual
register `stack0` (`[ESP+4]` at the export entry):

```powershell
.\fixtures\hook_syscall\run_cli_modify.ps1 -Arch x86
```

Hook events keep the four captured register values in `a0..a3` and expose the
pre-action stack snapshot in `a4..a7`; the runner reports the original `a4`
value alongside the target's post-action status.
