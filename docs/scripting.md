# 脚本系统（内嵌 CPython 多插件）

PinBridge 的动态脚本能力：CPython 3.10 解释器直接编进 `pinbridge_agent.dll`，任意数量的
Python 插件在运行时通过控制协议热加载/热卸载（哪怕目标正停在断点上），可以订阅事件流、下/删
断点与 hook 点、读写内存与寄存器、停走单步、符号解析与导出枚举——分析自动化的主战场。

权威实现：`pinbridge-agent/src/scripting/`（`api.rs` = pb 模块的全部 pyfunction,
`host.rs` = 宿主循环与派发，`mod.rs` = 插件注册表，`output.rs` = 输出环）。

## 架构

- **解释器在 agent 里，不再有独立插件 DLL**。pyo3 0.22  vendored 在
  `bindings/rust/third_party/pyo3`，带一处补丁：GIL 计数改用进程全局原子——agent 被 Pin
  私有映射，模块 TLS 索引永不分配，std `thread_local` 在里面不可用（连 pyo3 自己的 GIL
  refcount 也一样），补丁细节见 `third_party/pyo3/src/gil.rs` 的 PINBRIDGE PATCH 块。
- **python310.dll 对 Pin 不可见**：agent 用 `/DELAYLOAD:python310.dll` 链接（build.rs)，
  Pin 的工具加载器走导入表时根本看不到它；脚本线程启动时用 `LoadLibraryExW`
  （`LOAD_WITH_ALTERED_SEARCH_PATH`）从 agent 所在目录预加载（失败回退 PATH 上的
  `LoadLibraryW`），之后的 delay-load 解析直接命中这个已加载模块。
- **LNK1194 绕过**：delay-load 不覆盖 CPython 的 DATA 导入（异常单例、bool 类型、
  None/True/False 等 `__imp_<Name>` 指针单元格）。`scripting/python_data.rs` 自己定义这些
  单元格，预加载后用 `GetProcAddress` 逐个填充，首个 pyo3 调用之前完成。
- **线程纪律（神圣不可退）**：所有 Python 只跑在**一条专用脚本线程**（Pin 内部线程）上，
  永不进 Pin 分析回调。插件的每个 `pb.*` 动作都是一次到查询服务线程的 loopback RPC
  （微秒级，每次调用一条短连接——查询服务单线程，长连接会饿死 UI)。宿主 tick ~5ms：
  每 tick 最多一次 loopback 往返（拉事件页 ≤2048 条 + 停点快照）,**连接先断开，再跑
  Python 回调**（回调里自己拨 RPC)。洪流下 tick 自适应退到 40ms，丢弃是设计行为。
- 加载顺序：`script run` 只做到**编译**（语法错误立刻回传）；顶层代码和 `pb_init()` 在
  随后的第一个 tick 执行——这样顶层的 `pb.*` 调用面对的是一个空闲的查询服务。
- 调试启动（`PINBRIDGE_ENTRY_BP=1`，CLI/TUI 默认开启）会把这个执行 tick 闸住到主 PE
  正常入口断下为止；入口停住后才执行启动目录插件或此前提交的脚本。首次入口停点后闸门
  永久打开，目标恢复运行时仍可热加载/替换/卸载脚本。`--no-entry-bp` 原始运行模式不启用
  此闸门。
- agent 目录定位走 `GetMappedFileName` + `QueryDosDevice`（私有映射没有 PEB 模块项，
  `GetModuleHandleEx(FROM_ADDRESS)` 找不到）。

## 部署形态

和 `pinbridge_agent.dll` 同目录：

- `pinbridge.dll`（C++ ABI 桥）
- `python310.dll`(CPython 3.10)
- `python310.zip`（**可选**嵌入式标准库；存在即切 isolated 配置，module search path =
  zip + agent 目录，完全不需要系统 Python)

agent 的 build.rs 会把 `pinbridge.dll` 和 `python310.dll`（从 `PYTHON_SYS_EXECUTABLE` /
PATH 上的 python / 标准 per-user 安装目录找）自动摆到 agent 旁边，找不到只告警不报错。

**python310.dll 缺失时 agent 照常工作**，只是脚本功能不可用（日志一行提示，`script run`
报 "python unavailable")。

## 插件模型

- 任意数量 `.py` 插件并存：**单解释器、每插件独立模块命名空间**，注册表在脚本线程
  （plain static，无锁——只有脚本线程碰它）。
- **任意时刻热加载/卸载**，包括目标停在断点上的时刻；同名加载 = 替换（先调旧插件的
  `on_unload` 再退役）。
- `PINBRIDGE_AGENT_PLUGINS=<dir>`：启动时按文件名排序自动加载目录下全部 `*.py`，单个失败
  不影响其余。
- 插件异常只把该插件置为 error 状态（agent 和其他插件不死）,`script list` 看得到。

## 脚本 API v2(`import pb`)

### 注册函数（收窄/声明订阅；改的是"当前插件"的过滤器）

```python
pb.on_exception(codes=None)   # 收窄 on_exception 到指定异常码;None = 全部
pb.on_syscall(numbers=None)   # 收窄 on_syscall 到指定系统调用号;None = 全部
                              # 原生位图过滤取所有插件的并集,下个 tick 生效
pb.on_bp()                    # 订阅断点命中(定义了 on_bp_hit 即默认订阅,此为显式)
pb.on_modules()               # 模块加载/卸载(同理,定义回调即订阅;此为显式)
pb.watch(kinds, range=None, batch=None)  # on_event_batch 的订阅:kinds 为事件类名列表,
                              # range=(lo,hi) 按事件指令地址过滤,batch=每 tick 页上限
                              # (默认 512,最大 4096)
pb.unsubscribe()              # 清掉 watch(专用回调继续流)
# 别名:pb.subscribe = pb.watch;pb.log = pb.print
```

kinds 合法值:`hook`/`hook_regs`、`mem`/`memory`、`exec`、`branch`/`branch_edge`、
`syscall`、`ctx`/`context_change`、`module_load`、`module_unload`。

### 固定回调（模块顶层定义即自动发现）

| 回调 | 触发 | 参数 |
|---|---|---|
| `pb_init()` | 顶层跑完之后 | 无 |
| `on_exception(evt)` | context_change 异常事件 | `{tid, code, rip, reason}` |
| `on_syscall(evt)` | syscall 引擎事件 | `{number, phase, tid, args[6], retval}`(phase 0=entry 带六参,1=exit 带 retval) |
| `on_bp_hit(evt)` | 断点命中停下 | `{tid, addr, id}` |
| `on_module_load(evt)` | 镜像加载 | `{base, end, is_main, name}` |
| `on_module_unload(evt)` | 镜像卸载 | `{base, end=0, is_main, name}` |
| `on_event_batch(events, missed)` | watch 订阅的事件批 | events=list[dict],missed=游标间隙丢弃数 |
| `on_stop(tid, addr)` | 目标停下 | tid == -1 表示手动暂停(非断点) |
| `on_unload()` | 被替换/卸载时 | 无 |

**定义了回调但没显式注册 = 无过滤订阅**;`on_event_batch` 的默认 kinds =
`hook|memory|exec|branch`，默认 batch 512。

`on_event_batch` 的事件 dict:`seq kind kind_name tid addr a0..a7`，语义按 kind:

| kind | kind_name | addr | a0 | a1 | a2 | a3 | a4..a7 |
|---|---|---|---|---|---|---|---|
| 1 | hook_regs | 抓取点 IP | rcx/ecx | rdx/edx | r8/eax | r9/ebx | a4..a7 为 ABI 栈参数快照 |
| 2 | memory | 指令地址 | EA | size | access(0=读,1=写,2=第二读操作数) | — | — |
| 3 | exec | 指令地址 | — | — | — | — | — |
| 4 | branch_edge | 指令地址 | target | taken | — | — | — |
| 5 | syscall | — | number | phase(0=entry,1=exit) | entry:arg0 | entry:arg1 / exit:retval | entry:arg2..arg5 / exit:a4=errno |
| 6 | context_change | 异常 IP | reason(4=异常) | info(异常码) | ip | — | — |
| 7 | module_load | base | base | end | is_main | — | — |
| 8 | module_unload | base | base | — | — | — | — |

syscall 的 `number` 在 x86/x64 都是 0..0xfff 的本机序号；IA-32 Pin 原始值携带的
service-class 高位会在进入事件和过滤器前移除。entry/exit 通过线程 TLS 保持同一编号。

### 动作函数（每次调用一次 loopback RPC)

控制与状态：
- `pb.stop() / pb.resume() -> bool`;`pb.step(tid, over=False) -> bool`
- `pb.is_stopped() -> bool`;`pb.wait_stop(timeout_ms) -> bool`(5ms 轮询);`pb.sleep(ms)`
- `pb.hit() -> (tid | None, addr)`（造成当前停下的断点命中）

断点（64 槽）与 hook 点（4096 槽，命中产 kind-1 hook_regs 事件）:
- `pb.bp_set(addr) -> id | None`;`pb.bp_remove(id) -> bool`
- `pb.breakpoint(addr, callback, once=False, thread_id=None) -> id`：把精确断点绑定到
  当前插件的 Python 函数。命中时目标保持停止，回调收到 `{type,id,address,addr,tid,
  stop_generation,hits,arch,pointer_width,context_complete,registers}`；返回 `stay`、`resume`、
  `step_into` 或 `step_over`。不返回等同于 `stay`，回调异常也保持停止。
- `pb.breakpoint_remove(id) -> bool`：只删除当前插件对该断点的绑定。插件卸载时自动
  释放其全部绑定；多个插件可共享同一原生断点。
- `pb.hook_set(addr) -> bool`(False = 满了);`pb.hook_remove(addr) -> bool`;`pb.hook_clear() -> bool`
- `pb.hook_rule(addr, set_reg, set_value, match_reg=None, match_mask=0, match_value=0, thread_id=None)`：在 Hook 命中时由原生回调同步修改上下文寄存器；可选条件按寄存器掩码匹配。`stack0`/`stack1` 等虚拟寄存器表示 ABI 栈参数（x86 从 `[ESP+4]` 起，x64 从 `[RSP+0x28]` 起）。
- `pb.hook_rules_clear()`：清除修改规则但保留 Hook 点。规则执行在 Pin 应用线程，不调用 Python 热路径。

内存（写要求目标处于停止状态）:
- `pb.read_mem(addr, len) -> bytes | None`（单次 ≤1MB)
- `pb.write_mem(addr, data: bytes) -> int`（实际写入字节数，0 = 被拒）

寄存器（要求停止状态；18 个 GP 名：rax..r15、rip、rflags):
- `pb.get_reg(tid, name) -> int | None`;`pb.set_reg(tid, name, value) -> bool`

符号、导出与反汇编：
- `pb.resolve(addr) -> "mod!sym+0x.." | None`（含 IAT thunk 一层追踪）
- `pb.resolve_name("module!Export") -> int | None`
- `pb.exports(module) -> [(addr, name), ...]`（命名 PE32/PE32+ 导出，上限 8192)
- `pb.disasm(addr, count≤128) -> [(addr, size, kind, target, text), ...]`

枚举与策略：
- `pb.modules() -> [(base, end, is_main, name), ...]`;`pb.threads() -> [tid, ...]`
- `pb.counters() -> (total, dropped, capacity, [kind_counts])`(kind_counts 8 类）
- `pb.exc_policy(enabled, code=0) -> bool`（异常暂停策略；停止窗口按设计不精确）

轨迹录制（.pbtr，见 docs/taint-roadmap.md 第 2 层；与 64K 主环相互独立）:
- `pb.trace_start(path, kinds=None, range=None) -> bool`;kinds 名映射到录制档：
  `exec`→exec_bytes(9)、`memory`→mem_value(10)、`branch`→4（显式 `exec_bytes`/
  `mem_value` 亦可；`exec_plain`/`mem_plain` = 只抓地址的 3/2);range=(lo,hi) 缺省
  为全地址（洪流自戕，务必圈窗）；已在录制 = False("already recording")
- `pb.trace_start_spec(path, kinds=None, ranges=None, threads=None) -> bool`；`ranges` 为
  `(lo, hi)` 列表，`threads` 为 Pin thread id 列表。过滤在 native recorder 的 ring claim
  前执行；空线程列表表示全部线程。用它替代事后 `main-module-only` 清洗。
- `pb.trace_extend(ranges) -> bool`：录制进行中原子追加临时地址范围，现有 kind/thread
  过滤保持不变，并在 PBTR 中写入 scope marker。
- `pb.memory_region(address) -> (base, size, allocation_base, protect, state, type) | None`：
  查询 VirtualQuery 区域，脚本可据此识别私有可执行堆代码。
- `pb.trace_stop() -> (recorded, dropped)`（等 drain 追平，~5s 上限）
- `pb.trace_status() -> (active, recorded, dropped)`

`examples/python/trace_scope.py` 提供模块名/RVA/线程/断点触发的采集模板。
设置 `TRACE_MAX_EVENTS` 时，脚本会在主事件窗口达到阈值后停止 recorder 并暂停目标；
生产分析仍应检查 PBTR 的 `dropped` 和序列缺口。

输出：
- `pb.print(msg)`(= `pb.log`)→ 输出环，见下文"输出到 UI 的路径"。

## CLI 用法

```bash
pinbridge-cli --port 9011 script run probe.py        # 加载/替换(插件名 = 文件名)
pinbridge-cli --port 9011 script list                # 全部插件:name/state/delivered/dropped
pinbridge-cli --port 9011 script status              # list 的别名(兼容旧脚本)
pinbridge-cli --port 9011 script off probe.py        # 卸载指定插件
pinbridge-cli --port 9011 script off all             # 全部卸载
pinbridge-cli --port 9011 script output              # 输出环快照
pinbridge-cli --port 9011 script output --follow     # 持续跟随(500ms 轮询,一行一个 JSON)

pinbridge-cli --port 9011 exports ntdll.dll          # 模块导出枚举
pinbridge-cli --port 9011 hook 0x7ff..               # 下 hook 点
pinbridge-cli --port 9011 hookall ntdll.dll          # 按唯一地址批量 hook 全部导出
pinbridge-cli --port 9011 hooks                      # 列出 hook 点
pinbridge-cli --port 9011 hookdel 0x7ff..            # 删 hook 点
pinbridge-cli --port 9011 hookclear                  # 清空 hook 点
pinbridge-cli --port 9011 syscallfilter all          # 全量 syscall
pinbridge-cli --port 9011 syscallfilter only 0x55    # 只放行的号(原生位图)

pinbridge-cli --port 9011 trace start exec,mem_value 0xLO 0xHI C:\out.pbtr
                                                     # 开录(kinds: exec=exec_bytes,
                                                     # memory/mem_value, branch, 或裸 kind 号)
pinbridge-cli --port 9011 trace stop                 # 停录 -> recorded/dropped
pinbridge-cli --port 9011 tracest                    # 录制状态(active/recorded/dropped)
```

## KUED 异常接管模式

VMP 系保护壳把自己的异常导向 SEH 处理；经典解法是掐 `KiUserExceptionDispatcher`——所有
到达用户态的异常都先经过它，断在那里就是接管决策点：自己处理（dump/修复/重定向）或放行。
插件侧两条通道并排：`on_exception` 纯观察，KUED 上下 `bp_set` 真接管（目标被停住，插件
读写上下文后 `resume`)。骨架见 `examples/python/unpack_guard.py`。

## 输出到 UI 的路径

`pb.print` / 插件异常 / 生命周期事件（loaded、replaced、unloaded、编译错误）统一进 agent
内的**输出环**(4096 行，带单调 seq)，经 SCRIPT_OUTPUT op 对外分页；消费者：

- `pinbridge-cli script output [--follow]`;
- TUI 的 Plugins 面板（轮询 script_list + script_output，本地保留尾 1000 行）;
- agent 日志同时落一份 `[py:<plugin>] ...`(`pinbridge-agent.log`)。

## 线程与性能须知（血泪）

- Python 永不跑在 Pin 分析回调里；事件批量投递。脚本里每次 `pb.*` 是一次 loopback RPC
  （微秒级）。每秒几万到十几万条带逻辑的事件 Python 接得住；**指令级洪流请先原生过滤**——
  引擎开关（`engine KIND on|off`)、`PINBRIDGE_AGENT_RANGE`、watch 的 range、
  `syscallfilter only`（原生位图），别让洪流进 Python。
- 插件游标每 tick 从 64K 事件环翻页 ≤2048 条；默认引擎全开时 exec 洪流 ~100 万条/秒，
  翻页只是追赶。宿主按上一 tick 的实际 Python 开销自适应缩页/降频（5→40ms)。
- `on_stop` 在断点命中的 ~10ms 内触发；`pb.wait_stop` 是脚本自动化的核心节拍。
- "hook 全部 ntdll 导出"用 hook 点（4096 槽）不是断点（64 槽）——`exports` + 循环
  `hook_set` 即可，命中事件自带 RCX/RDX/R8/R9 四个 Win64 参数寄存器。

## 已知怪癖

1. **python 就绪竞态**：脚本功能约在端口绑定后 ~1s 才可用（预加载 + 解释器初始化在脚本
   线程上异步完成）;`script run` 报 "python unavailable" 时重试即可。
2. **部分稀有类事件仍走普通环**：生命周期、SMC、Pin 分离/附加和内存不足已经迁移到
   独立 4096 槽高优先级环；异常和模块事件暂时仍可能被默认引擎洪流（~100 万 exec/s）
   挤出 64K 普通环。等异常/syscall 时关掉用不到的引擎
   （`engine 2 off; engine 3 off; engine 4 off`)——`tests/control_e2e.py` 就是这么做的。
3. **异常码符号扩展**:`on_exception` 的 `code` 到达时是符号扩展的 64 位值
   （如 `0xFFFFFFFFC0000005`)，用前掩到 u32(`code & 0xFFFFFFFF`)。
4. **间歇性堆损坏崩溃**（历史遗留，排查中）：签名恒定为内部线程在 `ntdll.dll+0x5b897`
   （堆块头解码）读野指针；脚本负载下的触发率高于旧基线（~1/20)。`diag.rs` 的崩溃捕获器
   对 AV 类故障写 `crash_dump.txt`，复现先拿 dump。
5. **hook 别名去重**:ntdll 的 `Zw*`/`Nt*` 对共享地址，hook 集合按地址去重——
   `hooks` 的数量少于 `exports` 数量是去重，不是丢点。

## 统一事件订阅

新代码使用 `pb.on` 注册命名事件，旧的顶层固定回调继续兼容：

```python
import pb

def thread_started(event):
    pb.print("thread %d started at 0x%x" % (event["tid"], event["ip"]))

subscription_id = pb.on("thread.start", thread_started)
pb.off(subscription_id)
```

- `pb.on(name, callback, once=False) -> subscription_id`：当前插件订阅一个异步通知；
- `pb.off(subscription_id) -> bool`：只移除当前插件拥有的订阅；
- `pb.event_names() -> list[str]`：返回 `pb.on` 接受的规范事件名；
- 断点是会停止目标的同步事件，必须使用 `pb.breakpoint(address, callback)` 注册。

目前已接入的命名事件：`process.start`、`process.exit`、`thread.start`、
`thread.exit`、`module.load`、`module.unload`、`exception`、`context.change`、
`syscall`、`hook.entry`、`hook.return`、`instruction`、`memory`、`branch.edge`、
`code.smc`、`pin.detach`、`pin.attach`、`memory.oom`。

订阅 `instruction`、`memory`、`branch.edge` 或 `syscall` 会把对应的原生采集引擎加入
脚本需求并在下一次宿主节拍开启。取消订阅不会擅自关闭可能由 CLI/UI 开启的全局引擎。
`hook.entry`/`hook.return` 只观察已经用 `pb.hook_set`、`pb.hook_rule` 或 CLI 创建的 Hook
点；订阅本身不会猜测要 Hook 哪个地址。

所有处理函数接收一个字典。公共字段为 `type`、`sequence`、`kind`、
`kind_name`、`thread_id`/`tid`、`address`/`addr` 和 `a0..a7`。生命周期字段：

| 事件 | 专用字段 | 说明 |
|---|---|---|
| `process.start` | `phase="start"` | 插件晚于应用启动加载时，每个订阅补发一次当前状态 |
| `process.exit` | `phase="exiting"`, `exit_code`, `source` | 在用户态退出路径、Pin 最终销毁前通知 |
| `thread.start` | `ip`, `flags` | `tid` 是 Pin 线程号，回调不在该应用线程上运行 |
| `thread.exit` | `ip`, `exit_code` | 退出码按有符号 64 位值提供 |
| `code.smc` | `trace_start`, `trace_end` | 第一次订阅时才启用 Pin 的 SMC 跟踪 |
| `memory.oom` | `requested_size` | 原生分配失败通知；回调自身不分配内存 |
| `pin.detach` | `phase="detached"` | 已接入 JIT/Probe 原生完成回调；分离后不承诺 Python 仍被调度 |
| `pin.attach` | `phase="attached"` | 字段已固定；完整重新附加控制链仍在开发 |

原生生命周期回调只写固定大小记录，不分配内存、不获取 GIL，也不等待 Python。
Python 处理函数统一在脚本内部线程按“插件名、注册顺序”稳定调用。处理函数异常只把
所属插件置为 error，不会在 Pin 回调栈中传播到目标程序。

原生层在 `RtlExitUserProcess`/`ExitProcess` 入口提前产生 `process.exit`，使 Python 内部
线程仍有调度机会；`PrepareForFini` 是绕过常规退出 API 时的保底边沿。交接等待默认上限为 1000ms，可在启动前用
`PINBRIDGE_SCRIPT_EXIT_GRACE_MS=0..5000` 调整。超时后原生层无条件继续退出，Python
故障不会把被分析进程永久卡在结束阶段。

上述生命周期、SMC、Pin 分离/附加和内存不足事件使用独立 4096 槽高优先级环，先于
普通遥测派发。生产回调只执行固定记录和 try-lock，不调用 Python、不做阻塞等待。
`pinbridge-agent.log` 的 Fini 行提供 `priority_total` 和 `priority_dropped`。
