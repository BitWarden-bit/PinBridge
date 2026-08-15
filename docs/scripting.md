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
- 同名更新先编译、后替换：语法错误不会卸载正在运行的旧插件；同一 tick 内连续提交多个
  同名且可编译的版本时，只保留最后一份待执行代码，旧代码对象在持有 GIL 时释放。
- 事件游标在执行插件顶层代码之前建立；每次 `pb.on(...)` 又在注册语句执行的准确时刻记录
  普通环、高优先级环和观察环的边界。因此注册之前已经存在的记录不会误投给新处理函数，
  而注册之后、即使 `pb_init()` 尚未返回时到达的事件也不会被初始化收尾吞掉。粘性生命周期
  状态（例如 `process.start`）仍按契约补发，不受这条边界限制。
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
  `on_unload` 再退役），但只有新源码编译成功后才开始替换。
- `PINBRIDGE_AGENT_PLUGINS=<dir>`：启动时按文件名排序自动加载目录下全部 `*.py`，单个失败
  不影响其余。
- 插件异常只把该插件置为 error 状态（agent 和其他插件不死），`script list` 继续保留它供诊断。
  宿主随后统一隔离该插件：撤销高频原生策略，释放它拥有的精确断点以及每一份异步/同步
  Hook 租约，并重新计算原生过滤开关；错误插件不会留下仍能命中但已无人处理的原生点。

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
| `on_exception(evt)` | context_change 异常事件 | `{tid, code, rip, reason, exception_generation, context_generation}` |
| `on_syscall(evt)` | syscall 引擎事件 | `{number, phase, tid, args[6], retval, syscall_generation}`(phase 0=entry 带六参,1=exit 带 retval) |
| `on_bp_hit(evt)` | 断点命中停下 | `{tid, addr, id}` |
| `on_module_load(evt)` | 镜像加载 | `{base, end, is_main, name, module_generation}` |
| `on_module_unload(evt)` | 镜像卸载 | `{base, end=0, is_main, name, module_generation}` |
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
| 5 | syscall | syscall_generation | number | phase(0=entry,1=exit) | entry:arg0 | entry:arg1 / exit:retval | entry:arg2..arg5 / exit:a4=errno |
| 6 | context_change | 变化前 IP | reason(0..5) | info | from_ip | context_generation | a4=to_ip, a5=to_ip_known, a6=from_ip_known |
| 7 | module_load | base | base | end | is_main | module_generation | — |
| 8 | module_unload | base | base | — | — | module_generation | — |

syscall 的 `number` 在 x86/x64 都是 0..0xfff 的本机序号；IA-32 Pin 原始值携带的
service-class 高位会在进入事件和过滤器前移除。entry/exit 通过线程 TLS 保持同一编号。

### 动作函数（每次调用一次 loopback RPC)

控制与状态：
- `pb.stop() / pb.resume() -> bool`;`pb.step(tid, over=False) -> bool`
- `pb.is_stopped() -> bool`;`pb.wait_stop(timeout_ms) -> bool`(5ms 轮询);`pb.sleep(ms)`
- `pb.hit() -> (tid | None, addr)`（造成当前停下的断点命中）
- `pb.control_port() -> int`：当前 agent 的查询/Python 控制端口；
  `pb.parent_control_port() -> int | None`：跟随子会话的父端口，根会话为 `None`。
- `pb.pin_state() -> (state, registration_status)`；状态为 `attached`、`detach_requested`、
  `detached`、`attach_requested`、`attaching` 或 `attach_failed`。
- `pb.pin_attach_supported() -> bool`：当前 Pin 模式和平台是否支持进程内重新附加。
- `pb.pin_detach() -> bool`：异步请求 JIT/Probe 分离；完成后产生 `pin.detach`。
- `pb.pin_attach() -> bool`：从 `pin.detach` 处理函数请求重新附加；`True` 表示 Pin 已接受，
  `False` 表示分离尚未完成，需要稍后重试。桥接/状态错误抛出 `RuntimeError`。

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
- `pb.instrumentation_set(kinds=None, ranges=None, threads=None) -> generation`：替换当前插件
  的原生采集规则。`kinds` 可取 `instruction`、`instruction.decode`、`memory`、`branch.edge`、
  `trace.instrument`、`routine.instrument`、`basic_block.instrument`；`ranges` 是半开区间
  `(lo, hi)` 列表，省略时使用启动配置的 trace range；`threads` 省略或空列表表示全部线程。
- `pb.instrumentation_policy() -> (kinds, ranges, threads) | None`：读取当前插件自己的规则。
- `pb.instrumentation_clear() -> bool`：移除当前插件的规则，不影响其他插件。

高频插桩 API 是同一 DLL 内的控制面更新，不发 loopback RPC。Python 只配置不可变规则；
Pin 插桩回调和分析回调在原生层执行种类、范围和线程过滤。规则更新会使相关 Pin 代码
缓存失效，因此已运行过的函数也能按新规则重新插桩。各插件规则保持独立的“且”关系后
再按“或”合并；插件卸载或进入 error 状态时自动撤销其规则。每个插件最多 64 个范围、
64 个线程号，所有运行插件合计最多 64 个合并前范围。
- `pb.memory_translation_set(mappings, threads=None, instruction_ranges=None, operations=None,
  include_pin=False) -> generation`：替换当前插件的地址映射。每项 mapping 是
  `(source_start, source_end, target_start)`，命中后保持区间偏移；operations 可取 `load`、
  `store`，其他两个列表省略或为空表示不限制。
- `pb.memory_translation_policy() -> (mappings, threads, instruction_ranges, operations,
  include_pin) | None`：读取当前插件自己的地址转换规则。
- `pb.memory_translation_clear() -> bool`：移除当前插件规则，其他插件不受影响。

地址转换的源区间在当前插件内、以及所有运行插件之间都不得重叠；冲突更新失败并回滚。
一次访存必须完整落在源区间内才转换，跨界访问保持原地址。默认不转换 Pin 自身访问；
原子读改写按 `store` 分类。规则发布后，原生层让所选指令范围重新 JIT，并用两个 Pin 工具
寄存器承接转换结果、改写应用的真实内存操作数。热路径不运行 Python。
- `pb.code_fetch_set(segments) -> generation`：替换当前插件预置的机器码段。每项是
  `(virtual_address, bytes)`；发布后旧、新范围的 Pin 翻译缓存立即失效。
- `pb.code_fetch_policy() -> [(virtual_address, bytes), ...] | None`：读取当前插件策略。
- `pb.code_fetch_clear() -> bool`：移除当前插件的机器码段，不影响其他插件。

机器码段在插件内部和所有运行插件之间都不得重叠；合计最多 64 段、1 MiB。Pin 取码
回调命中段时直接复制不可变原生快照，未命中部分通过 `PIN_SafeCopyEx` 语义读取原应用
字节；热路径不运行 Python、不分配、不加锁。插件卸载或进入 error 状态时自动撤销其段。
取码器只在第一次非空策略时注册，但 Pin 没有注销接口；此后 `clear` 是全量原始取码透传。
自定义取码器启用后 Pin 不再保证自动发现全部 SMC，平台只自动失效本 API 更新涉及的
范围；目标自行改写的其他代码需要脚本显式重新发布策略。

XED 解码配置：
- `pb.xed_decode_set(cet=None, cldemote=None, mpx=None) -> generation`：设置当前插件明确
  指定的解码输入。`True`/`False` 都是明确值，`None` 表示本插件不决定该项。
- `pb.xed_decode_policy() -> (cet, cldemote, mpx) | None`：读取当前插件自己的配置。
- `pb.xed_decode_clear() -> bool`：撤销当前插件的配置。

Pin 的 XED 回调发生在解码前，不是“已解码指令通知”。平台在该回调里只读取原子快照并
设置 CET、CLDEMOTE、MPX 输入；借用的 XED 指针不会进入 Python。解码语义是进程级的，
所以多个运行插件对同一项给出相反明确值时，更新失败并回滚。策略变化会让 Pin 全局翻译
缓存失效，以便旧代码按新输入重新解码。

已解码通知使用另一条正确的链路：先订阅 `pb.on("instruction.decode", callback)`，或定义
`on_event_batch` 并调用 `pb.watch(["instruction.decode"], range=...)`；再用
`pb.instrumentation_set(kinds=["instruction.decode"], ranges=[...])` 做原生地址过滤。
事件在线程无关的插桩阶段产生，因此 `tid == -1`，`threads` 过滤不适用于该种类。专用字段
为 `size`、`category`、`extension`、`opcode`、`memory_operand_count`、
`has_fall_through`、`is_branch`、`is_call`、`is_return`、`is_syscall`。这些值已经从 INS/XED
对象复制到固定记录；回调需要文本时可在脚本线程按地址调用 `pb.disasm`。

函数、Trace 和基本块同样使用“Python 声明、原生产生、脚本线程批量消费”的模型。先用
`pb.on` 或 `pb.watch` 订阅对应名字，再把相同种类加入 `pb.instrumentation_set`。函数策略
启用时会枚举已加载模块的函数，弥补脚本热加载晚于模块加载的问题；以后 Pin 新发现的函数
继续由 RTN 回调投递。Trace 和基本块在代码缓存创建/重新翻译时投递，因此同一地址可能因
失效重编译出现多次，`policy_generation` 用于区分策略版本。这三类是静态插桩事件，
`tid == -1`，`threads` 过滤不适用；`ranges` 仍在原生回调内精确执行。函数名和模块名不从
借用的 Pin 对象跨线程复制，脚本需要文本时按 `address` 调用 `pb.resolve`。

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
  （`instrumentation_set/clear/policy` 是同 DLL 内规则更新）。每秒几万到十几万条带逻辑的
  事件 Python 接得住；**指令级洪流请先原生过滤**——优先用
  `pb.instrumentation_set` 的种类/范围/线程规则，也可用引擎开关（`engine KIND on|off`)、
  `PINBRIDGE_AGENT_RANGE`、watch 的 range、
  `syscallfilter only`（原生位图），别让洪流进 Python。
- 插件游标每 tick 从 64K 事件环翻页 ≤2048 条；默认引擎全开时 exec 洪流 ~100 万条/秒，
  翻页只是追赶。宿主按上一 tick 的实际 Python 开销自适应缩页/降频（5→40ms)。
- `on_stop` 在断点命中的 ~10ms 内触发；`pb.wait_stop` 是脚本自动化的核心节拍。
- "hook 全部 ntdll 导出"用 hook 点（4096 槽）不是断点（64 槽）——`exports` + 循环
  `hook_set` 即可，命中事件自带 RCX/RDX/R8/R9 四个 Win64 参数寄存器。

## 已知怪癖

1. **python 就绪竞态**：脚本功能约在端口绑定后 ~1s 才可用（预加载 + 解释器初始化在脚本
   线程上异步完成）;`script run` 报 "python unavailable" 时重试即可。
2. **异常码符号扩展**:`on_exception` 的 `code` 到达时是符号扩展的 64 位值
   （如 `0xFFFFFFFFC0000005`)，用前掩到 u32(`code & 0xFFFFFFFF`)。
3. **间歇性堆损坏崩溃**（历史遗留，排查中）：签名恒定为内部线程在 `ntdll.dll+0x5b897`
   （堆块头解码）读野指针；脚本负载下的触发率高于旧基线（~1/20)。`diag.rs` 的崩溃捕获器
   对 AV 类故障写 `crash_dump.txt`，复现先拿 dump。
4. **hook 别名去重**:ntdll 的 `Zw*`/`Nt*` 对共享地址，hook 集合按地址去重——
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

- `pb.on(name, callback, once=False, address=None, numbers=None) -> subscription_id`：当前插件订阅
  一个异步通知；`address` 只适用于 `hook.entry/hook.return`，`numbers` 只适用于 `syscall`；
- Hook 指定 `address` 时会自动复用或挂载原生点，并在 `pb.off`、`once` 完成或插件卸载时按
  所有权计数释放；省略地址仍可观察脚本或 CLI 已经挂载的全部 Hook 点；
- 常驻 syscall 观察应显式传 `numbers=[...]`；`numbers=None` 会接收全部系统调用，独立环也会
  在 Python 长期处理不过来时覆盖旧记录并计入丢失；
- `pb.off(subscription_id) -> bool`：只移除当前插件拥有的订阅；
- `pb.event_names() -> list[str]`：返回 `pb.on` 接受的规范事件名；
- 断点是会停止目标的同步事件，必须使用 `pb.breakpoint(address, callback)` 注册。

目前已接入的命名事件：`process.start`、`process.exit`、`process.prepare_fini`、`thread.start`、
`thread.exit`、`module.load`、`module.unload`、`exception`、`context.change`、
`syscall`、`hook.entry`、`hook.return`、`instruction`、`instruction.decode`、`memory`、`branch.edge`、
`code.smc`、`pin.detach`、`pin.attach`、`memory.oom`、`pin.internal_exception`、
`debugger.breakpoint`、`debugger.single_step`、`debugger.async_break`、
`trace.instrument`、`routine.instrument`、`basic_block.instrument`。

订阅 `instruction`、`memory`、`branch.edge` 或 `syscall` 会把对应的原生采集引擎加入
脚本需求并在下一次宿主节拍开启。取消订阅不会擅自关闭可能由 CLI/UI 开启的全局引擎。
`pb.on("hook.entry/return", callback, address=...)` 会直接创建地址绑定的观察点；不传
`address` 时只观察已经用 `pb.hook_set`、`pb.hook_rule`、同步拦截器或 CLI 创建的 Hook 点。

所有处理函数接收一个字典。公共字段为 `type`、`sequence`、`kind`、
`kind_name`、`thread_id`/`tid`、`address`/`addr` 和 `a0..a7`。生命周期字段：

| 事件 | 专用字段 | 说明 |
|---|---|---|
| `process.start` | `phase="start"` | 插件晚于应用启动加载时，每个订阅补发一次当前状态 |
| `process.exit` | `phase="exiting"`, `exit_code`, `exit_code_known`, `source` | 在用户态退出 API 入口尽早通知；未捕获该入口时由准备结束边沿尽力补发 |
| `process.prepare_fini` | `phase="prepare_fini"`, `exit_code`, `exit_code_known`, `had_exit_request`, `trigger`, `native_prepare_reached` | 可执行 Python 清理的预收尾窗口；正常 Windows 路径的 `trigger="exit_api"`，发生在真正的 Pin PrepareForFini 之前 |
| `thread.start` | `ip`, `flags` | `tid` 是 Pin 线程号，回调不在该应用线程上运行 |
| `thread.exit` | `ip`, `exit_code` | 退出码按有符号 64 位值提供 |
| `module.load` | `base`, `end`, `is_main`, `name`, `module_generation` | 高优先级环和兼容普通环双写；同一原生代号对每个处理函数只投递一次 |
| `module.unload` | `base`, `name`, `module_generation` | 与加载事件共享单调递增的原生代号；真实 DLL 卸载回归已覆盖 |
| `exception` | `reason`, `reason_name`, `code`, `ip`, `exception_generation`, `context_generation` | 异常边沿向高优先级环和兼容普通环双写；两个 generation 在异常事件中相同 |
| `context.change` | `reason`, `reason_name`, `info`, `from_ip`/`ip`, `from_ip_known`, `to_ip`, `to_ip_known`, `context_generation`, `exception_generation` | 六类 Pin 原因全部双写；只有异常原因的 `exception_generation` 非零 |
| `syscall` | `number`, `phase`, `args`/`retval`/`errno`, `syscall_generation` | `phase` 为 `enter`/`exit`；原生号码过滤后向 16384 槽观察环和兼容普通环双写并逐处理函数去重 |
| `hook.entry` | `registers`, `stack_arguments` | 可按绝对地址自动挂载并过滤；快照在同步规则/拦截修改现场前产生 |
| `hook.return` | `return_value`, `registers`, `stack_arguments` | 地址指向 `ret` 指令；`return_value` 是同步修改前的 `rax/eax` |
| `code.smc` | `trace_start`, `trace_end` | 第一次订阅时才启用 Pin 的 SMC 跟踪 |
| `memory.oom` | `requested_size`, `occurrence`, `recovered_from_emergency_slot` | 原生先追加 `pinbridge_oom.log`，存活时再通知 Python；`recovered_from_emergency_slot=True` 表示普通高优先级环未作为唯一投递来源 |
| `pin.internal_exception` | `ip`, `code`, `exception_address`, `fault_address`, `fault_address_known`, `access_type`, `exception_class` | 先写原生崩溃记录；只有 Pin 仍存活时 Python 才可能收到 |
| `pin.detach` | `phase="detached"` | 已接入 JIT/Probe 原生完成回调；分离后不承诺 Python 仍被调度 |
| `pin.attach` | `phase="attached"` | 支持的平台在全部会话回调重建并再次 application-start 后投递；Pin 3.31 Windows JIT 不支持重附加 |
| `debugger.breakpoint` / `debugger.single_step` / `debugger.async_break` | `ip`, `debugging_event`, `stack_pointer`, `flags`, `return_value` | Pin 准备把停止事件报告给应用调试器时产生；这里只观察，不改变处理结果 |
| `trace.instrument` | `size`, `basic_block_count`, `instruction_count`, `has_fall_through`, `routine_address`, `policy_generation` | Pin 为执行路径创建动态 Trace 时产生 |
| `routine.instrument` | `size`, `instruction_count`, `routine_id`, `is_dynamic`, `is_artificial`, `policy_generation` | 策略启用时补当前函数快照，随后接收 Pin 新发现函数 |
| `basic_block.instrument` | `size`, `instruction_count`, `has_fall_through`, `is_original`, `policy_generation` | Trace 内的基本块静态元数据 |

原生生命周期回调只写固定大小记录，不分配内存、不获取 GIL。除退出交接窗口外不等待
Python；退出交接也只有有界确认等待。Python 处理函数统一在脚本内部线程按“插件名、
注册顺序”稳定调用。处理函数异常只把
所属插件置为 error，不会在 Pin 回调栈中传播到目标程序。

原生层在 `RtlExitUserProcess`/`ExitProcess` 入口先产生 `process.exit`，确认派发后再产生
`process.prepare_fini`，因此 Python 清理代码在 Pin 停止内部脚本线程前实际执行。随后真正
的 Pin PrepareForFini 只设置原生确认位，最终 Fini 写事件和总结日志；这两个最终阶段不
虚报为仍能运行 Python。绕过常规退出 API 时，原生 PrepareForFini 仍会尽力投递两个选择器，
但 Windows 不保证脚本线程还能调度。每个 Python 交接阶段的等待上限默认 1000ms，可在
启动前用 `PINBRIDGE_SCRIPT_EXIT_GRACE_MS=0..5000` 调整；正常路径最坏合计为两倍该值。
超时后原生层无条件继续退出，Python 故障不会把被分析进程永久卡在结束阶段。

上述生命周期、模块加载/卸载、全部上下文变化、SMC、Pin 分离/附加、内存不足、Pin 内部异常和调试器事件使用独立 4096 槽高优先级环，先于
普通遥测派发。生产回调只执行固定记录和 try-lock，不调用 Python；仅上述退出交接使用
有上限的确认等待。
模块事件仍同步写入普通环，保持 CLI/UI 和旧 `on_event_batch` 兼容；两份记录携带同一个
`module_generation`，命名回调和旧固定回调各自去重，不会因为双写而调用两次 Python。
所有上下文变化采用相同的兼容设计，两份记录携带同一个 `context_generation`；异常事件
同时把该值作为 `exception_generation`。每个 `pb.on("exception")`、
`pb.on("context.change")` 处理函数和旧 `on_exception` 回调分别使用固定 65536 位窗口去重，
所以三种 API 可以同时使用，多应用线程乱序也不会误删真实事件或收到双份事件。
系统调用命名回调和旧 `on_syscall` 使用独立的 16384 槽原生过滤观察环，不与稀有事件共享
容量，也不会被 instruction/memory 洪流覆盖。兼容普通环仍保留给 CLI/UI 和
`on_event_batch`；双写记录共享 `syscall_generation`，每个 Python 处理函数各自去重。
不同应用线程可能乱序写入事件环，因此这里不是只记“最大代号”，而是每个处理函数使用
固定 65536 位滑动窗口；乱序的真实事件不会被误删，状态大小也不会随运行时间增长。
命名 Hook 观察也使用该 16384 槽观察环；原生点本身就是第一层地址过滤，每个处理函数再按
自己的 `address` 精确匹配。Hook 的普通环副本仅供 CLI/UI 和 `on_event_batch` 兼容消费，
不会再次路由到命名回调，因此同一次原生采集只调用一次对应 Python 处理函数。同步和异步
Hook 订阅共用地址租约，任一 `once` 处理函数完成都不会提前拆除其他订阅仍使用的点。
内存不足另有不分配 Rust 堆、不加锁的紧急路径：先用预先转换好的固定文件名追加
`pinbridge_oom.log`，再发布一个原子保底槽并尝试写高优先级环。脚本宿主先处理当次可用的
环记录，缺失时读取保底槽，并按 `occurrence` 去重迟到的环记录，因此同一次原生回调只调用一次 Python。并发 OOM
回调不会互相等待；保底槽正被写入的事件仍保留独立紧急日志和高优先级环尝试。
`pinbridge-agent.log` 的 Fini 行提供 `priority_total/priority_dropped`、
`observation_total/observation_dropped` 和 `oom_total`。
自动测试覆盖紧急行格式、保底槽/环去重和事件字段；真实耗尽内存可能破坏测试机稳定性，
因此不把它列为已强制触发的回归。

## 同步决定：子进程跟随

普通 `pb.on` 回调只观察事件。原生行为必须等待 Python 返回值时，使用单独的同步决定
注册表：

```python
import pb

def decide_child(event):
    pb.print("child pid=%d argv=%r" % (event["pid"], event["argv"]))
    return {"follow": "--analyze-child" in event["argv"]}

def pb_init():
    decision_id = pb.intercept("child.follow", decide_child, once=False)
```

- `pb.intercept(name, callback, once=False) -> decision_id`：注册当前插件拥有的同步处理函数；
- `pb.unintercept(decision_id) -> bool`：删除当前插件的同步处理函数；
- `pb.decision_names() -> list[str]`：返回所有同步决定名；
- 回调返回 `bool` 或 `{"follow": bool}`；多处理函数采用“全部同意才跟随”；
- 事件字段为 `type`、`generation`、`process_id`/`pid`、`argv`、`argv_bytes`、为子会话预分配的
  `control_port`，以及当前父会话的 `parent_control_port`；
- 默认等待上限为 2000ms，可用 `PINBRIDGE_SCRIPT_DECISION_TIMEOUT_MS=1..10000` 调整；
- 无处理函数、Python 未就绪/忙碌、捕获失败、异常、非法返回值和超时都不跟随。

Pin 回调只复制固定上限的 PID/命令行并等待 semaphore，不获取 GIL；Python 始终在专用
脚本线程执行。决定处理期间可以使用 `pb.print`，但不能调用需要查询服务或目标停止状态的
`pb.*` 动作，这些调用会快速返回失败以避免同步回调与查询服务互相等待。

真实回归入口是 `fixtures/child_follow_demo/run.ps1`，`-Follow $false` 和
`-Follow $true` 都必须通过。Fini 日志给出 `child_decisions`、`child_follow`、
`child_reject`、`child_decision_timeouts` 和 `child_config_failures`。跟随决定为真时，脚本线程
先选择一个空闲回环端口，再把完整 Pin 子命令行写入固定槽；等待中的 Pin 回调只调用
`CHILD_PROCESS_SetPinCommandLine`，不分配内存。子 agent 在 `PIN_Init` 前读取并剥离内部端口
参数，以独立端口启动查询服务，同时给继承的日志文件名加入子 PID/端口后缀，避免覆盖父日志。
子脚本可以用 `pb.control_port()` 和 `pb.parent_control_port()` 查看拓扑。真实跟随回归会连接
该子端口并热加载第二个 Python 插件，不再只验证“Pin 说它跟随了”。

### 同步 Hook

```python
def entry(event):
    # event: type/id/generation/tid/address/registers/arguments
    if event["registers"]["rcx"] == 5:
        return {"action": "return", "return_value": 0x1234}
    return None

entry_id = pb.intercept("hook.entry", entry, address=api_address, once=True)

def returned(event):
    return {"return_value": 0x5678}

return_id = pb.intercept("hook.return", returned, address=ret_address)
```

`address` 是必填的非零绝对地址，`thread_id` 可选。注册会自动复用或创建 4096 点原生
Hook 集合中的对应点；订阅卸载时按所有权计数释放。返回字典支持：

- `registers={"rax": value, ...}`：按当前 x86/x64 架构名修改通用寄存器；
- `arguments=[a0, ... a3]`：只在 `hook.entry` 修改前四个 ABI 栈参数；Win64 的前四个
  寄存器参数直接通过 `registers` 的 `rcx/rdx/r8/r9` 修改；
- `return_value=value`：修改 `rax`/`eax`；
- `action="continue"`（默认）或仅入口可用的 `action="return"`/`"skip"`。

同步 Hook 不停止整个进程，也不占断点槽；命中的应用线程在 16 槽固定通道上限时等待
脚本线程。槽满、超时、Python 不可用、返回结构错误或多插件补丁冲突时继续原上下文。
决定回调里 `pb.print` 可用，普通 `pb.*` 目标 RPC 会快速失败。真实回归入口为
`fixtures/hook_python_demo/run.ps1`，同时验证入口跳过、返回值修改、地址绑定异步观察的
`once`/常驻语义，以及两类订阅在相反释放顺序下都不会互相提前拆除。

### 同步系统调用

```python
def before(event):
    args = list(event["arguments"])
    args[0] = 0
    return {"arguments": args}

entry_id = pb.intercept("syscall.entry", before, numbers=[0x0F], once=True)

def after(event):
    return {"return_value": 0xC0000022, "errno": 0}

exit_id = pb.intercept("syscall.exit", after, numbers=[0x0F])
```

- `numbers=[...]` 是原生号码过滤；省略表示全部，生产脚本应避免全量同步等待；
- `thread_id` 可选；`address` 不适用于 syscall；
- entry 可返回 `number` 和最多六项 `arguments`；
- exit 可返回 `return_value` 和 `errno`；
- `None` 表示不修改，插件可在回调中调用 `pb.unintercept(id)` 结束常驻拦截；
- 同一字段的多插件返回冲突、异常、超时或槽满时保留原上下文。

同步过滤与异步观察分开管理。命名接口使用
`pb.on("syscall", callback, numbers=[...])`，旧固定回调继续用 `pb.on_syscall(numbers=[...])`
声明号码；所有运行插件的号码取并集后编译成原生位图。真实回归入口为
`fixtures/syscall_python_demo/run.ps1`：第一轮在 entry 把 `NtClose` 句柄改为无效值，
第二轮让内核真正关闭句柄，再在 exit 修改 NTSTATUS；同一用例按 `syscall_generation`
验证命名/旧回调各自无重复、两者事件集合一致且独立观察环丢失为 0。

### 同步异常接管

```python
def recover(event):
    # from_registers: 异常发生现场（只读快照）
    # registers: Pin 即将切换到的目标现场
    return {"registers": {"rip": recovery_address}}

decision_id = pb.intercept(
    "exception.handle",
    recover,
    codes=[0xC0000005],
    thread_id=None,
    once=True,
)
```

- `codes=[...]` 是原生异常码过滤；省略表示全部异常；
- `thread_id` 可选，`address` 和 `numbers` 不适用；
- 事件字段为 `type/id/generation/tid/thread_id/address/addr/reason/code`，以及
  `from_registers`、`registers` 两份按架构命名的寄存器字典；
- 返回 `None` 表示保持目标上下文，返回 `{"registers": {name: value}}` 修改指定寄存器；
- 未知寄存器、返回结构错误、回调异常、超时、槽满或多插件字段冲突时，本次不应用任何
  Python 补丁，继续操作系统原有异常处理路径；
- 回调在脚本线程执行，可以 `pb.print`，不能发起需要查询服务的目标 RPC。

直接把 `rip/eip` 改到函数入口不等同于执行一次 `call`，栈布局仍由脚本负责。真实 x64
回归 `fixtures/exception_python_demo/run.ps1` 触发访问违规，回调根据 `from_registers`
构造 Win64 栈并改写 `rip/rsp`，目标跳到恢复入口、绕过原生 SEH 处理器并以 0 退出；同一
回归先投递 Windows APC，验证非异常 `context.change` 精确一次，再验证
`pb.on("exception")`、`pb.on("context.change")` 和旧 `on_exception` 的异常观察各精确一次。

### 同步调试器事件

Pin 在把应用断点、单步或异步中断报告给已连接的应用调试器前调用拦截器。观察现场使用
`pb.on(...)`；需要暂停该应用线程、等待 Python 返回决定时使用同名 `pb.intercept(...)`：

```python
def breakpoint_for_debugger(event):
    regs = event["registers"]
    if regs["rax"] == 0x1234:
        # 吞掉这个调试器断点，修改现场后直接恢复应用线程。
        return {
            "pass_to_debugger": False,
            "registers": {"rax": 0, "rip": recovery_address},
        }
    # 不修改现场，仍让调试器正常停住。
    return None

decision_id = pb.intercept(
    "debugger.breakpoint",
    breakpoint_for_debugger,
    thread_id=None,
)
```

- 可拦截名为 `debugger.breakpoint`、`debugger.single_step`、`debugger.async_break`；
- 事件包含 `type/id/generation/tid/thread_id/address/addr/registers`，寄存器按当前 x86/x64
  架构命名；只接受可选的 `thread_id` 过滤；
- 返回 `None` 默认继续交给调试器；返回字典可带 `pass_to_debugger: bool` 和
  `registers: {name: value}`，也可用 `action="pass"` / `action="squash"`；
- Pin 明确禁止吞掉 `debugger.async_break`。断点或单步继续交给调试器时也禁止修改
  `rip/eip`；要修改指令指针，回调必须在同一个字典中明确返回
  `pass_to_debugger=False`；
- 多插件对去向或同一寄存器给出冲突值、Python 异常、超时、同步槽满或返回值非法时，
  本次不应用任何寄存器补丁，并继续把事件交给调试器；
- 原生回调不获取 GIL。只有原生兴趣位命中时，该应用线程才在固定同步槽上等待脚本线程，
  等待上限仍由 `PINBRIDGE_SCRIPT_DECISION_TIMEOUT_MS` 控制。

这里的真假方向来自 Pin 原始接口：`true` 表示“继续报告给调试器”，`false` 表示“吞掉并恢复
线程”。平台使用完整字段名 `pass_to_debugger`，不使用含义容易相反的 `handled`。当前已完成
原生三类回调注册、优先级观察事件、同步现场复制、返回校验和上下文写回；自动测试覆盖 ABI
契约与 Rust 逻辑。连接 WinDbg/GDB 的交互回归需要独立调试器测试环境，当前不虚报为已通过。

高频规则的真实回归入口为 `fixtures/instrumentation_python_demo/run.ps1`。测试先在
`PINBRIDGE_AGENT_ENGINES=none` 下执行并缓存目标函数，再热加载 Python 规则，只允许一个
函数范围内的 `instruction` 事件；通过第二次调用证明 Pin 已动态重新插桩，并验证旁边的
排除函数没有事件泄漏。

地址转换真实回归入口为 `fixtures/memory_translation_python_demo/run.ps1`：Python 把源
变量映射到 backing 变量，只允许指定函数的 `load`；目标输出同时证明映射读取已生效、
未匹配的物理访问仍留在源地址。

机器码取码真实回归入口为 `fixtures/code_fetch_python_demo/run.ps1`：目标先执行并缓存返回
`1` 的原函数，握手后 Python 预置另一函数的字节，第二次调用返回 `2`，同时覆盖运行期
注册、旧翻译失效、不可变原生快照和未映射地址安全回退。
