# PinBridge

Windows x64 动态二进制分析平台,基于 Intel Pin 3.31:冻结的 **C ABI**(697 个导出)
把 Pin 的 C++ API 暴露给任何语言;其上是用 Rust 写的**调试 agent**(PinTool)——
事件引擎、断点/单步、异常与 syscall 观测、全速 trace 录制——并**内嵌 CPython**,
让分析逻辑成为任意时刻可热加载的 Python 插件;重分析(污点、反混淆)以
**录制 → 离线重放**的方式在纯 Python 里完成。

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│ 前端:pinbridge-cli(JSON 子命令) / pinbridge-tui / pinbridge-ui │
│        或任何 TCP 客户端(AI、MCP、你自己的工具)                 │
├─────────────────────────────────────────────────────────────┤
│ loopback 二进制查询协议(pinbridge-proto / pinbridge-client)    │
├─────────────────────────────────────────────────────────────┤
│ pinbridge_agent.dll(Rust PinTool)                            │
│  事件引擎 exec/memory/branch/hook_regs/syscall/module → ring  │
│  断点(精确停泊) 单步 异常观测 模块事件 导出枚举 hook 点集        │
│  trace 录制(独立大 ring + 落盘 .pbtr,ABI v1.5 档2 捕获)       │
│  内嵌 CPython 3.10 多插件宿主(docs/scripting.md)              │
├─────────────────────────────────────────────────────────────┤
│ pinbridge.dll:冻结 C ABI v1.10(697 个 pb_* 导出)             │
│  Pin 3.31 公共 SDK 的 C 包装:无 C++/异常/STL 跨界,opaque     │
│  handle,(buffer,capacity,required_size*) 三段式,无跨界所有权   │
└─────────────────────────────────────────────────────────────┘
```

## 能力一览

- **控制面**:stop/resume、单步 si/so(落点回调内停泊,精确)、instrumentation 断点
  (64 槽)、内存读写、18 个 GP 寄存器读写、模块/线程/导出枚举、符号解析
  (含 IAT thunk 追踪)、反汇编
- **观测面**(原生全速,实测 1360 万事件/1.5s):exec / memory(EA+大小+读写) /
  branch / hook_regs(抓 RCX/RDX/R8/R9)/ syscall(entry/exit 六参+返回值) /
  context_change / module_load/unload;运行时引擎开关、地址范围过滤、
  syscall 号码位图过滤、4096 点运行时 hook(不吃断点槽,"hook 全部 ntdll 导出")
- **Python 插件**(核心):任意时刻热加载/卸载/同名替换(目标停在断点上也行);
  `pb.on` 统一异步事件、`pb.breakpoint` 精确停点处理、`pb.intercept` 同步原生决定
  （子进程、Hook、syscall、异常、调试器事件）;完整动作 API;`pb.print` 输出直达 CLI/TUI
- **录制与离线重放**:`trace start/stop` 录窗口(档2:指令字节 + 内存读写值),
  `.pbtr` 落盘;`examples/python/replay/` 纯 Python 重放库:capstone 解码、
  字节级影子内存、**前向污点 + 反向切片**(具体 EA 消别名)
- **示例插件**(`examples/python/`):api_trace(按名下断打参数)、ntdll_trace
  (全导出 hook 上 UI)、syscall_watch(号码发现)、oep_watch(写后执行)、
  unpack_guard(KUED 异常接管脱壳骨架)

## 快速开始

构建(需要 VS + CMake + Rust + Pin 3.31 SDK):

```powershell
.\Build-Pin.ps1 -Configuration Release -PinRoot <PIN_SDK_ROOT>
cd bindings\rust; cargo build --release --workspace
# 产物:target\release\pinbridge_agent.dll(+自动 staged 的 pinbridge.dll、python310.dll)
```

跑起来:

```powershell
$env:PINBRIDGE_AGENT_PORT = "9011"
<PIN_SDK_ROOT>\intel64\bin\pin.exe -t target\release\pinbridge_agent.dll -- C:\Windows\System32\hostname.exe
# 另一个终端:
.\pinbridge-cli.exe --port 9011 ping
.\pinbridge-cli.exe --port 9011 modules
```

第一个插件(30 秒):

```python
# hello.py
import pb

def pb_init():
    addr = pb.resolve_name("ntdll.dll!NtCreateFile")
    pb.print(f"NtCreateFile @ {addr:#x}")
    pb.hook_set(addr)

def on_event_batch(events, missed):
    for e in events:        # hook_regs: a0..a3 = RCX/RDX/R8/R9
        pb.print(f"hit tid={e['tid']} rcx={e['a0']:#x}")
    pb.hook_clear()         # 打一次就够
```

```powershell
.\pinbridge-cli.exe --port 9011 script run hello.py     # 任意时刻注入,~5ms 生效
.\pinbridge-cli.exe --port 9011 script output --follow  # 看插件输出
.\pinbridge-cli.exe --port 9011 script off              # 卸载
```

录制 + 污点重放:

```powershell
.\pinbridge-cli.exe --port 9011 trace start exec,memory 0x140000000 0x140100000 C:\tmp\win.pbtr
.\pinbridge-cli.exe --port 9011 trace stop
cd examples\python\replay
python taint.py C:\tmp\win.pbtr forward --source mem:0x140020000:0x100
python taint.py C:\tmp\win.pbtr slice --at 12345 --operand reg:rdx
```

## 文档

- `docs/scripting.md` — Python 插件 API 权威参考、部署、事件字典、已知怪癖
- `docs/taint-roadmap.md` — 录制/重放分析路线(为什么是 record→replay)

## 测试

```powershell
.\Run-Tests.ps1                    # CMake 契约 60/60 + 绑定生成 + 导出双向校验(697)
$env:PINBRIDGE_PIN_EXE = "<PIN_SDK_ROOT>\intel64\bin\pin.exe"
python tests\control_e2e.py        # 控制面真机 e2e
python tests\script_e2e.py         # 多插件脚本真机 e2e(12 步)
python tests\stress_control.py     # 防卡死 200 循环(+ VMP 目标 30 循环)
python examples\python\replay\test_taint.py   # 重放单测 16/16
fixtures\child_follow_demo\run.ps1 -Follow $false  # 子进程不跟随决定
fixtures\child_follow_demo\run.ps1 -Follow $true   # 子进程跟随决定
fixtures\hook_python_demo\run.ps1                  # Hook 入口跳过 + 返回值同步修改
fixtures\syscall_python_demo\run.ps1               # syscall 入口参数 + 出口状态同步修改
fixtures\exception_python_demo\run.ps1             # 异常现场交给 Python 改写后恢复执行
fixtures\instrumentation_python_demo\run.ps1       # 动态重新插桩 + 指令/函数/Trace/基本块生命周期
fixtures\memory_translation_python_demo\run.ps1    # Python 映射地址并由原生层改写真实访存
fixtures\code_fetch_python_demo\run.ps1            # Python 预置机器码并触发原生重新取码
fixtures\xed_decode_python_demo\run.ps1             # XED 预解码策略 + 已解码命名/批量事件
fixtures\pin_reattach_python_demo\run.ps1           # 重附加能力探测；Windows JIT 安全拒绝而不杀目标
```

## 项目结构

```
include/pinbridge/    冻结 ABI 头(pinbridge.h + generated/*.inc)
src/                  facade(pinbridge_*.cpp) + backend 接口 + Pin 实现
msvc/                 PinTool DLL 工程(PinCRT、/GR- /EHs-)
bindings/rust/        Rust workspace:sys(生成 FFI) tool proto client agent tui ui cli
                      third_party/pyo3(vendored,含补丁说明见 docs/scripting.md)
tools/                绑定生成器(generate_rust_bindings.py)
tests/                ABI 契约(C,mock backend)+ 真机 e2e(python)+ ps1 辅助
examples/python/      插件示例 + replay/(录制重放库)
docs/                 scripting.md / taint-roadmap.md
```

## 限制与 backlog

- 平台:Windows x64 / Pin 3.31 (build 98869) / Python 3.10;Linux 平台层未做
- 已知怪癖见 `docs/scripting.md` 末章(稀有类事件在 exec 洪流下会被挤掉——
  等异常/syscall 前先关 exec 类引擎;异常码需 mask 到 u32;……)
- Backlog:MCP 服务接入(作为客户端接查询协议)、runtime parity 编译期对拍、
  ABI 账本生成器

本仓是早前内部原型的最底层 ABI 抽取 + 重写,当前为独立仓库。
