# fixtures/x86 — 32-bit PE fixture

`hello32.c` is a tiny, self-contained 32-bit target for pinbridge tests. It is
isolated from the rest of the repo: it reads no files, takes no arguments, and
never touches a debugger or an external sample. Its only output is one line on
stdout reporting the result of a byte-reversal and a rotate-and-XOR checksum
plus a small branch classification.

```text
hello32: input=pinbridge-ia32 reversed=23ai-egdirbnip checksum=... class=N
```

## Build

Run from this directory (or anywhere — the script resolves its own path):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File fixtures\x86\build.ps1
```

The script probes for a 32-bit C compiler in this order:

1. **MSVC x86 `cl.exe`** — found via `vswhere` (needs the
   `Microsoft.VisualStudio.Component.VC.Tools.x86.x64` component, i.e. the
   "MSVC v143 x86/x64 build tools" workload), then run inside
   `VsDevCmd.bat -arch=x86`.
2. **`clang-cl.exe`** with `--target=i386-pc-windows-msvc`.
3. **`gcc.exe -m32`** (MinGW-w64 i686).

Exit codes:

| Code | Meaning                                                        |
|------|----------------------------------------------------------------|
| 0    | `hello32.exe` built and verified as PE32/I386                  |
| 77   | **SKIP** — no usable 32-bit C compiler; nothing was produced   |

`hello32.exe` is never fabricated: it only appears when a real compiler
succeeded *and* the result passed a header check, so the file on disk is
always a genuine PE32.

## Expected PE headers

Once built, `hello32.exe` must be a classic PE32 image:

| Field                | Offset                    | Expected value        |
|----------------------|---------------------------|-----------------------|
| DOS magic            | 0x00                      | `MZ`                  |
| `e_lfanew`           | 0x3C                      | → `PE\0\0` signature  |
| COFF `Machine`       | `e_lfanew + 4`            | `0x014C` (`I386`)     |
| Optional `Magic`     | `e_lfanew + 24`           | `0x010B` (`PE32`)     |

That `Machine`/`Magic` pair is exactly what
`pinbridge_client::arch::parse_pe` reads to classify the target as `x86`.

## Handing it to pinbridge

**Architecture detection** (`--arch auto` reads the PE headers, never the file
name):

```powershell
# In the CLI, auto-detection happens during `run` resolution:
pinbridge-cli --arch auto --pin <ia32 kit>\pin.exe --agent <ia32 agent dir>\pinbridge_agent.dll `
    run -- fixtures\x86\hello32.exe
```

Because the fixture is PE32/I386, `auto` selects the `ia32` Pin runtime and the
`ia32/pinbridge_agent.dll` agent — it will never silently fall back to the
`intel64` kit.

**Trace** — once the backend is up (or against an already-running session on
the default port), record the buffer/reversal code and dump events:

```powershell
pinbridge-cli --port 9001 trace start exec,memory,branch 0x400000 0x410000 C:\tmp\hello32.pbtr
# ... let the target run to completion, then:
pinbridge-cli --port 9001 trace stop
pinbridge-cli --port 9001 events --limit 16
```

The entry-point stop (`--entry-bp`, default on) gives a deterministic point to
set breakpoints or arm the trace before the first instruction runs.

## ia32 Python 回归

仓库的 Rust 构建脚本会准备匹配架构的 CPython embeddable 运行时并构建带脚本宿主的 x86
agent。以下测试在真实 ia32 Pin 下停在 PE 入口，由 x86 Python 读取 EIP、反汇编并注册下一
落点的精确断点；回调必须收到 `arch="x86"`、`pointer_width=4` 和完整 x86 寄存器后恢复目标：

```powershell
cd bindings\rust
.\build-agents.ps1 -BuildX64 $false
cd ..\..
.\fixtures\x86\run_python.ps1
```

## Limits without an ia32 runtime

`hello32.exe` builds and its PE headers are fully verifiable on any machine
with an x86 compiler, but **在 Pin 下运行仍要求 ia32 运行时**：

- an ia32 Pin kit (`ia32/bin/pin.exe`), and
- an ia32 bridge build (`build/pin/ia32/Release/pinbridge.dll`)；Rust agent 和匹配的 x86
  CPython 可由 `bindings/rust/build-agents.ps1` 生成。

If either is missing, `pinbridge-cli run` fails with a descriptive `x86`
architecture error instead of faking i686 support. The PE-parser unit tests
still validate the fixture's headers (`Machine=0x014C`, `Magic=0x010B`)
without any Pin runtime present, and skip cleanly when `hello32.exe` has not
been built.
