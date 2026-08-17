# Python 事件平台设计

本文定义 PinBridge 的 Python 事件模型。实现必须遵守本文中的线程、所有权、停止状态和兼容性约束，避免把新的事件类型继续堆入单一派发函数。

## 目标

Python 是分析平台的控制面：脚本负责注册事件、设置过滤条件、读取事件现场并返回处理决定；Pin/Rust 原生层负责捕获事件、维护目标停止状态、验证修改并执行决定。

“由 Python 操作”不等于从 Pin 回调线程直接执行 Python。所有 Python 代码仍只在专用脚本线程运行。

## 三种执行模型

### 停止事件

断点、单步落点和以后支持的同步拦截属于停止事件。目标已停止，脚本可以安全地读取和修改停止上下文。所有注册处理函数执行完以后，宿主只执行一次最终决定：

- `stay`：保持停止；
- `resume`：恢复运行；
- `step_into`：单步进入；
- `step_over`：单步越过。

处理函数不返回值时等同于 `stay`。处理函数抛出异常时插件进入错误状态，目标保持停止。

### 高优先级通知

进程/线程/模块生命周期、异常边沿、动态代码修改等低频事件进入独立的高优先级队列。原生回调只复制固定大小的数据，Python 随后处理；只有退出前的两阶段交接使用有上限的确认等待，其余异步事件不在 Pin 回调线程等待 Python。异常观察同时保留兼容遥测记录并按原生代号去重，同步异常接管则使用独立固定槽。

### 普通遥测

指令执行、内存访问、分支、解码和插桩元数据属于高频遥测。它们必须经过原生过滤并批量交给 Python。队列满时记录明确的丢失计数，不能阻塞目标线程。

### 原生过滤观察

系统调用、Hook 等中等频率事件不能占用稀有事件队列，也不能被无关指令遥测覆盖。Python
先声明号码或 Hook 地址等选择条件，原生层过滤后写入独立 16384 槽观察环，同时保留普通环兼容记录；系统调用两份
记录用同一原生代号逐处理函数去重。每个处理函数持有固定 65536 位滑动窗口，允许多应用
线程乱序发布代号，同时避免去重状态无限增长。生产者仍然只尝试 Pin mutex，拥塞通过
独立计数暴露。常驻脚本必须按号码收窄；全量订阅在 Python 消费不足时仍会发生明确计数的
观察环覆盖。

## 断点订阅接口

第一阶段提供以下接口：

```python
breakpoint_id = pb.breakpoint(
    address,
    callback,
    once=False,
    thread_id=None,
)

pb.breakpoint_remove(breakpoint_id)
```

每个插件拥有自己的断点处理函数。多个插件可以订阅同一原生断点地址；底层断点只在最后一个订阅者离开后移除。

处理函数接收一个事件字典：

```python
{
    "type": "breakpoint",
    "id": 7,
    "address": 0x140001000,
    "tid": 3,
    "stop_generation": 12,
    "context_complete": True,
    "registers": {
        "rax": 0,
        "rbx": 0,
        "rip": 0x140001000,
        "rflags": 0x202,
    },
}
```

`registers` 是宿主在调用任何 Python 处理函数前取得的一次性现场快照。处理函数仍可使用 `pb.read_mem`、`pb.write_mem`、`pb.get_reg` 和 `pb.set_reg` 做进一步操作。

处理函数可返回字符串：

```python
return "stay"
return "resume"
return "step_into"
return "step_over"
```

也可以返回字典，为以后增加寄存器补丁、内存补丁和诊断信息保留稳定入口：

```python
return {"action": "resume"}
```

第一阶段寄存器和内存修改继续通过现有 `pb.set_reg` / `pb.write_mem` 完成。宿主不会在一个隐式返回值中偷偷修改目标。

## 多插件决定规则

一次停止可能命中多个插件对同一地址的订阅。宿主按插件名和注册顺序稳定调用所有匹配处理函数，然后合并决定：

1. 任意处理函数返回 `stay`，最终保持停止；
2. `step_into` 与 `step_over` 冲突时保持停止并记录错误；
3. 全部明确返回 `resume` 时才自动恢复；
4. 没有匹配的新式处理函数时，不执行自动动作，交给旧接口处理。

Python 处理函数不能通过返回值绕过其他插件。旧脚本直接调用 `pb.resume()` 的行为为了兼容仍然保留，但新脚本应使用返回动作。

## 所有权与卸载

- `pb.breakpoint` 创建的订阅属于当前插件；
- 插件替换、卸载或初始化失败时，宿主自动释放该插件的订阅；
- 顶层、`pb_init()`、普通事件、断点或同步决定处理函数异常时，插件保留为可诊断的 error
  条目，但只隔离一次：清空全部事件/决定/策略所有权，并按实际订阅数释放重复的 Hook 租约；
- 相同地址在底层复用一个断点编号；
- 只有不存在其他新式订阅时才移除底层断点；
- 旧 `pb.bp_set` / `pb.bp_remove` 保持原有全局语义，不与新接口混用所有权。

## 兼容接口

以下接口继续保留：

- `pb.bp_set`、`pb.bp_remove`；
- 顶层 `on_bp_hit(event)`；
- 顶层 `on_stop(tid, address)`。

旧回调收到的字段和停止行为不改变。新式断点处理完成后，旧 `on_bp_hit` 和 `on_stop` 仍能收到通知；如果新式处理已经恢复目标，宿主不得再次恢复或单步。

## 断点与单步的原生边界

- 普通断点由断点表拥有，Python/CLI 订阅决定它的生命周期；单步落点使用独立的最多三个
  后继地址槽，取消或完成单步绝不能删除普通断点。
- 单步请求记录目标 Pin 线程编号。其他线程即使先执行到同一后继地址，也不能完成这次
  单步；真正落点仍在指令执行前通过精确停泊交给 Python。

### 目标程序自己的 TF 单步异常

目标通过 `POPF/POPFD/POPFQ` 设置 TF 时，处理规则与平台单步严格分离。PinBridge 在 JIT
代码缓存真正设置物理 TF 之前模拟 POPF 的用户态标志语义，执行下一条应用指令后，使用
`PIN_RaiseException` 在应用上下文重新产生 `EXCEPTION_SINGLE_STEP (0x80000004)`：

- 默认交给目标程序原有 VEH/SEH，不得按 Pin 内部崩溃处理；
- 同一事件仍进入 context-change 高优先级通道，可被 Python 观察或按显式策略接管；
- 平台断点/单步由自己的所有权表处理，不会误投递给目标；
- 重投递前先清除原生 pending 状态，避免异常递归。

覆盖测试 `pb_single_step_fixture` 会在运行时生成
`pushfq; or [rsp],0x100; popfq; nop; ret`，校验 VEH 只收到一次异常且 RIP 位于 RET。
- 当前停止指令恢复时只抑制目标线程的一次重放。自环跳转第二次回到同一地址必须被视为
  真实落点，不能被重复抑制。
- x64 流程解码使用 XED 64 位长模式和 RAX/RIP 等寄存器；x86 使用 XED 32 位传统模式和
  EAX/EIP 等寄存器，地址计算按 32 位回绕。`0x40` 在 x86 中必须解码为 `INC EAX`，不能
  被当作 x64 REX 前缀。

### 通用执行区间监控

`pb.execution_trap(start, end, once=True, thread_id=None)` 把半开地址范围发布给原生引擎。
匹配指令尚未执行时，引擎复用精确断点的全局停泊路径；只有目标进入稳定停止状态后，才
向专用脚本线程投递 `execution.trap`。事件包含监控编号、范围、命中地址/线程、命中次数和
停止代号。Python 不进入 Pin 插桩或分析回调，常规指令洪流也不会进入事件队列。

该能力故意没有 `oep_arm` 或 `vmp_*` 原生接口。OEP、解密完成、壳版本与 Dump 都是上层
策略语义，应由外置 Python 根据系统调用、内存属性、模块和现场组合判断。PinBridge 核心
只保证通用监控的精确性、线程语义、资源所有权和插件卸载清理。

## 模块边界

- `scripting/subscriptions.rs`：订阅数据、断点所有权和动作类型；
- `scripting/api.rs`：Python 参数校验和注册入口；
- `scripting/host.rs`：解释器生命周期、异步事件循环和断点停止快照；
- `scripting/instrumentation.rs`：合并各插件拥有的高频采集规则并发布原生快照；
- `scripting/memory_translation.rs`：地址映射所有权、冲突检查和原生转换快照；
- `scripting/native_policies.rs`：插件失败或卸载时统一刷新全部原生策略；
- `execution_trap.rs`：通用执行区间表、原生匹配和精确停机后的高优先级事件发布；
- `scripting/interceptors/`：同步决定总调度，以及相互独立的
  `child`、`hook`、`syscall`、`exception` 处理与补丁合并；
- `context.rs` / 查询协议：提供一次性寄存器快照；
- `bp.rs`：只负责原生断点表、命中和停止，不保存 Python 对象。

原生断点引擎永远不依赖 Python。禁用脚本功能的 x86 构建继续保留断点、控制面和轨迹能力。

## 后续事件接入顺序

断点模型验证稳定后，按同一注册表接入：

1. 线程创建/退出、应用启动、结束前准备和最终退出；
2. 动态代码修改、Pin 分离/重新附加、内存不足；
3. 子进程跟随和调试器决策；
4. Hook 入口/返回、系统调用和异常的同步处理；
5. 地址转换、取指和插桩规则的“Python 配置、原生执行”接口。

每一类事件都必须有：事件结构、线程语义、是否允许阻塞、失败默认值、队列策略、丢失统计以及 x86/x64 测试，不能只增加一个回调名字。

## 已完成：统一异步订阅与生命周期事件

第二阶段不再继续增加互不相关的顶层函数名，而是提供统一注册表：

```python
sid = pb.on("thread.start", callback, once=False)
pb.off(sid)
```

`scripting/events.rs` 是公开事件名、选择器和事件字典结构的唯一来源；
`scripting/host.rs` 只负责调度；`lifecycle.rs` 只负责 Pin 原生回调和固定记录。
每个插件可以为同一事件注册多个处理函数，按照注册顺序调用，插件卸载时订阅随插件
对象一起释放。

宿主在运行插件顶层代码和 `pb_init()` 之前先建立插件级读取游标；每个 `pb.on(...)` 处理函数
再在注册语句执行时分别记录普通、高优先级和原生过滤观察通道的准确边界。这样既能读取
初始化期间新产生的记录，又不会把注册之前的旧记录误投给新处理函数。固定旧式回调在
`pb_init()` 返回、即将发布回调指针时建立自己的代号边界；断点停止代号不会在初始化结束时
重置。粘性生命周期补发和内存不足保底通知是合成状态，按各自的发生代号去重，不套用环游标。

应用启动是粘性状态：脚本通常在目标入口停住后才加载，因此 `process.start` 会对每个
新订阅补发一次。`process.exit` 在 `RtlExitUserProcess`/`ExitProcess` 入口提前投递；
确认完成后，同一安全窗口再投递独立的 `process.prepare_fini`，让 Python 清理逻辑确实
能在 Pin 停止内部线程前运行。两个阶段分别等待脚本线程确认，默认各最多 1000ms，超时
后继续退出。真实 Windows 回归同时验证了两个 Python 回调、目标正常退出，以及随后原生
PrepareForFini 确实到达。真正的 PrepareForFini/Fini 只设置原生状态、写最终事件和总结
日志，不虚假承诺 Python 能在最终销毁阶段运行；绕过常规退出 API 的异常退出路径只能在
原生 PrepareForFini 尽力补发，Windows 不保证此时脚本线程仍会被调度。

线程和应用回调遵守同一个硬边界：Pin 回调只读取上下文指令地址并提交 POD 事件，绝不
直接调用 Python。系统调用命名订阅会参加原生开关合并，没有脚本订阅时不为接口表象
而额外开启事件流。

## 已完成：高优先级事件通道

第三阶段增加了独立于 64K 遥测环的 4096 槽高优先级环。进程/线程/模块生命周期、
全部上下文变化、`code.smc`、`pin.detach`、`pin.attach`、`memory.oom`、`pin.internal_exception` 和三类调试器事件使用自己的游标，脚本宿主每个
节拍先处理高优先级记录，再处理原生过滤观察、断点和普通遥测。因此指令或内存事件洪流不会再把这些
记录从普通环中挤掉。

高优先级原生生产者仍然不能直接调用 Python，也不能等待普通互斥锁：它们只做固定大小
记录和 Pin mutex 的 try-lock。锁竞争导致的丢失计入 `priority_dropped`，在 Fini 总结
日志中可见；环覆盖造成的游标缺口计入插件的 `dropped`。

模块加载/卸载为兼容已有 CLI/UI 和 `on_event_batch`，同时写入高优先级环与普通环。两份
POD 记录共享 `module_generation`；每个 `pb.on` 处理函数以及旧固定模块回调分别保存自己的
已投递代号，因此两种 Python API 可以共存，而同一个 API 不会收到双份事件。真实 Pin
回归会加载并卸载专用 DLL，验证名称解析、加载/卸载字段和两类事件各精确一次。

六类上下文变化都写入高优先级环和兼容普通环，两份 POD 记录共享单调递增的
`context_generation`。`pb.on("exception")`、`pb.on("context.change")` 的每个处理函数与旧
`on_exception` 分别持有固定乱序窗口，因而可以同时观察同一次异常，并且各自只回调一次。
字典还提供原因名称以及变化前/后 IP 与有效位。真实 x64 Pin 回归先投递 Windows APC，再
触发访问违规；前者验证非异常变化，后者验证三类异常观察并由同步通道改写 `rip/rsp` 恢复。

`memory.oom` 在高优先级环之外还有两层专用保障。原生回调首先以固定栈缓冲和原始 Win32
写入追加 `pinbridge_oom.log`，这条路径不进行 Rust 堆分配、不获取锁，文件名也已在正常
初始化阶段转换完成；随后发布包含 `occurrence/requested_size` 的原子保底槽，再以 try-lock
尝试写高优先级环。进程还能调度脚本线程时，宿主先处理当次可用的环记录，缺失时再读取
保底槽，并用 `occurrence` 对迟到的同一记录去重。Python 字典中的
`recovered_from_emergency_slot` 标明当前投递确实来自该保底路径。
并发 OOM 不在原生回调中等待；保底槽碰到正在写入时，该次事件仍有紧急日志和环写入尝试。
Fini 总结同时记录 `oom_total`。

`code.smc` 在第一个 Python 订阅出现时才注册，因为 Pin 会从注册后开始维护 SMC 跟踪
状态，长期全局开启可能无限增长。运行中注册时由宿主持有 Pin client lock。真实 Pin
测试会执行、修改并再次执行一段动态机器码，验证 Python 收到 `trace_start`/
`trace_end`。

JIT 和 Probe 的分离完成回调在 C ABI v1.6 中严格分开：
`pb_pin_add_detach_function` 对应 JIT，`pb_pin_add_detach_function_probed` 对应 Probe；
C 后端和 agent 都按当前模式选择，避免把 Probe 回调注册到 JIT 启动路径。

Probe 是面向 VMP 等异常敏感目标的兼容执行模式，不是 JIT 的同功能别名。Agent 在该模式
只注册 Fini、模块、应用启动和 probed detach 回调；内存转换、XED、内部异常、系统调用、
上下文变化、调试器及逐指令/Trace 回调不会注册。这样目标保持原生异常语义，控制端口和
Python 宿主仍可使用，但脚本不能在 Probe 会话中要求断点、单步或指令级策略。

平台现在提供 `pb.pin_state/pin_attach_supported/pin_detach/pin_attach`。初始会话和重新附加
共用同一份注册清单；支持重新附加的平台会重新注册指令、函数/Trace/基本块、异常、系统
调用、模块、线程、调试器、子进程、内存转换、取码、XED 和已启用的 SMC 回调，同时保留
Python 插件和不可变策略快照。第二次 application-start 后才产生 `pin.attach`。

这里有不能隐藏的 Intel Pin 运行时限制：Pin 3.31 在 Windows JIT 模式调用 `PIN_Attach`
会直接终止目标并报告 “Re-Attach ... is NYI”。C 桥接因此在进入 Pin 前返回
`PB_ERR_UNSUPPORTED`，Python 可先用 `pb.pin_attach_supported()` 查询。真实 Windows 回归
已验证安全拒绝且目标正常退出；完整往返只能在支持 JIT reattach 的平台或适用的 Probe
工具中验证，目前不虚报为已通过。`memory.oom` 已用单元和契约测试覆盖紧急记录格式、
保底槽/环去重及公开字段；真实耗尽目标内存不是常规回归测试的安全做法，因此不虚报为
真实 OOM 回归已通过。

## 已完成：有返回值的同步决策通道

`pb.on` 只负责异步通知，返回值不会改变目标行为。必须由 Python 决定原生控制流的事件
使用独立接口，避免把“看见事件”和“接管事件”混在一起：

```python
def follow_child(event):
    return event["argv"][-1] == "instrument-me"

decision_id = pb.intercept("child.follow", follow_child, once=False)
pb.unintercept(decision_id)
```

`pb.decision_names()` 返回当前支持的同步决定名；第一项是 `child.follow`。事件字典包含
`type="child.follow"`、`generation`、`process_id`/`pid`、按 UTF-8 宽松解码的 `argv`
以及保留原始数据的 `argv_bytes`，并提供预分配的 `control_port` 和
`parent_control_port`。处理函数必须返回 `bool` 或
`{"follow": bool}`。

Pin 的跟随子进程回调不调用 Python。原生层在 `CHILD_PROCESS` 句柄仍有效时，把 PID 和
命令行复制到一个固定槽（最多 64 个参数、总计 8192 字节），然后使用 Pin semaphore
最多等待 2000ms。脚本线程领取副本、调用 Python、发布决定；Pin 回调只读取最终布尔值。
可在启动前用 `PINBRIDGE_SCRIPT_DECISION_TIMEOUT_MS=1..10000` 调整等待上限。

失败策略固定为“不跟随”：Python 未就绪、已有决定正在处理、参数超限、没有处理函数、
返回类型错误、处理函数异常或超时都返回 false。多个插件同时匹配时按插件名和注册顺序
调用，只有全部明确返回 true 才跟随。处理同步决定期间，`pb.print` 可用；会访问目标或
查询服务的 `pb.*` RPC 会快速失败，因为 Pin 回调正在等待决定，此时反向 RPC 可能死锁。

真实 Pin 回归已经覆盖返回 false 和返回 true 两条路径，并在 Fini 日志校验
`child_decisions`、`child_follow`、`child_reject`、`child_decision_timeouts` 和
`child_config_failures`。脚本线程在调用决定处理函数前从回环接口选择子端口；决定为真时，
它预先构造一份包含子/父端口的 Pin 命令行固定快照。等待中的 Pin 回调只把该快照交给
`CHILD_PROCESS_SetPinCommandLine`。子 agent 在自己的 `PIN_Init` 前剥离两个内部参数，优先
使用子端口而不是继承的 `PINBRIDGE_AGENT_PORT`，并派生独立日志名。`pb.control_port()` 返回
当前会话端口，`pb.parent_control_port()` 在根会话返回 `None`、在跟随子会话返回父端口。
真实跟随测试会连接子端口、热加载子插件并验证该 Python 拓扑，然后才允许子目标退出。

### Hook 同步拦截

Hook 的异步观察直接绑定地址；注册会自动创建或复用原生点：

```python
pb.on("hook.entry", observe_entry, address=entry_address)
pb.on("hook.return", observe_return, address=return_instruction, once=True)
```

异步快照进入独立观察环，普通环副本只服务 CLI/UI/批处理，不会再次触发命名回调。需要在
原指令执行前取得 Python 返回值时使用：

```python
pb.intercept("hook.entry", on_entry, address=entry_address, once=False)
pb.intercept("hook.return", on_return, address=return_instruction)
```

同步 Hook 使用独立的 16 槽固定 rendezvous，不占用 64 个断点槽。Hook 分析回调复制通用
寄存器和前四个 ABI 栈参数，脚本线程执行回调，原生层最后一次性应用返回补丁。返回值为
`None` 或字典；字典可包含 `registers={...}`、`arguments=[...]`、`return_value=...`。
`hook.entry` 还接受 `action="return"`：原生层从栈顶取得返回地址、弹出返回地址并跳回
调用者，因此可以跳过原函数；`hook.return` 的 `return_value` 在 `ret` 执行前修改返回
寄存器。

同一地址可以有多个插件处理函数。补丁字段相同且值一致时合并；同一字段出现不同值、
回调异常或非法返回结构时，本次拦截不应用任何 Python 补丁并继续原上下文。处理期间
目标线程在 Pin 回调中限时等待，所以回调只能计算、使用 `pb.print` 并返回补丁，普通
目标 RPC 快速失败。异步观察与同步拦截共用地址所有权计数；插件卸载、替换、`once`
完成、`pb.off` 或 `pb.unintercept` 会释放相应所有权。若注册前 Hook 点不存在，并且全部
脚本订阅都已释放，则延迟移除该原生点。

真实 Pin 测试同时覆盖入口直接返回（证明原函数未执行）和返回指令改写返回值；Fini
日志使用 `sync_decisions`、`sync_timeouts`、`sync_busy` 证明原生采用了三次 Python 决定。
测试目标通过握手文件在订阅全部注册后立即命中第一次 Hook，同时让 `pb_init()` 故意继续
运行 500ms；一次性异步观察仍必须收到这次初始化窗口内的命中，以回归验证精确注册边界。

### 系统调用同步拦截

系统调用入口和出口使用同一个固定同步通道，并必须尽量按号码收窄：

```python
pb.intercept("syscall.entry", on_entry, numbers=[nt_close_number])
pb.intercept("syscall.exit", on_exit, numbers=[nt_close_number])
```

入口事件包含 `number`、`arguments`（六项）、`standard`、`tid` 和回调现场地址；返回
`{"number": n, "arguments": [...]}` 可修改即将进入内核的系统调用号和参数。出口事件
增加 `return_value`、`errno`，返回同名字段可在应用继续前修改结果。`thread_id` 可进一步
限制线程；`numbers=None` 表示全部系统调用，但只适合短时诊断，不能当作常驻配置。

号码过滤在原生不可变快照中执行，不匹配的 syscall 不进入同步槽。异步观察使用
`pb.on("syscall", callback, numbers=[...])` 或旧固定回调配合 `pb.on_syscall(numbers=[...])`；
所有插件的号码取并集后在原生层过滤，匹配事件进入独立 16384 槽观察环并保留兼容遥测副本。
两份记录共享 `syscall_generation`，每个命名处理函数和旧固定回调独立去重。即使异步引擎
关闭，同步拦截仍然有效。多个处理函数的同一
字段必须返回相同值，否则本次保留原系统调用上下文。真实 Windows/Pin 测试用 `NtClose`
验证入口参数被替换、内核副作用被阻止，以及出口状态被改写为 `0xC0000022`；同一真实
回归还验证两类异步接口的原生代号集合一致、各自没有重复且 `observation_dropped=0`。

### 异常同步接管

普通 `pb.on("exception", ...)` 只观察异常；需要在异常转入系统处理器前修改 Pin 的目标
上下文时使用同步接管：

```python
def recover(event):
    # from_registers 是异常发生现场；registers 是 Pin 即将采用的目标现场。
    return {"registers": {"rip": recovery_address}}

pb.intercept(
    "exception.handle",
    recover,
    codes=[0xC0000005],
    thread_id=None,
    once=True,
)
```

事件包含 `type/id/generation`、`tid/thread_id`、`address/addr`、`reason`、`code`、
`from_registers` 和 `registers`。`codes` 在原生不可变快照中预过滤；省略表示全部异常，
`thread_id` 可进一步限制线程。回调返回 `None` 或 `{"registers": {...}}`，只有返回字典中
明确给出的通用寄存器会写回 Pin 提供的 `to` 上下文。

Pin 上下文切换回调不获取 GIL：它复制固定大小的源/目标寄存器，使用同一个 16 槽同步
通道限时等待脚本线程，再一次性应用补丁。无处理函数、Python 未就绪、槽满、超时、非法
寄存器、回调异常或多插件字段冲突时都不修改目标上下文，操作系统原有异常路径继续执行。
如果把指令指针直接改到普通函数，脚本还必须按目标 ABI 构造正确的栈；接口不会虚构一次
`call`。真实 x64 Pin 测试触发访问违规，Python 同时改写 `rip/rsp`，跳到恢复入口并绕过
原生 SEH 处理器；目标最终正常退出，日志证明 1 次同步决定、0 超时、0 槽位拥塞。

### 调试器事件：观察与同步决定分离

Pin 的调试器拦截回调只有三类：应用断点、单步和异步中断。平台为每类事件同时提供两条
独立路径：

```python
# 异步观察：不拥有控制流，返回值被忽略。
pb.on("debugger.breakpoint", log_debug_break)

# 同步决定：命中的应用线程限时等待 Python。
pb.intercept("debugger.breakpoint", decide_debug_break, thread_id=None)
```

原生回调先把 `tid`、指令指针、栈指针、标志和返回寄存器复制到高优先级环，再检查一个
三位原子兴趣掩码。没有同步订阅时立即返回，不进入同步槽；有订阅时复制当前架构的全部
通用寄存器，通过现有 16 槽 rendezvous 交给脚本线程。借用的 Pin `CONTEXT*` 不跨线程，
Python 只接触值快照，原应用线程醒来后才由原生层写回经验证的补丁。

同步回调返回 `None` 或：

```python
{
    "pass_to_debugger": False,       # False=吞掉并恢复线程；True=让调试器停住
    "registers": {"rip": next_ip},
}
```

失败默认值固定为 `pass_to_debugger=True` 且不写寄存器。多个处理函数必须对显式去向和同一
寄存器值达成一致，否则整次补丁作废。Pin 的原始契约还有两条强制限制：异步中断不能被
吞掉；断点/单步继续交给调试器时不能改 `rip/eip`。Python 参数校验和原生写回层各检查
一次，避免脚本错误违反 Pin 约束。

这类调试器事件不同于 `pb.breakpoint`：后者是平台自己的精确停点和脚本自动化机制；前者
只在 Pin 准备与外部应用调试器交互时发生。当前自动测试覆盖 C ABI 注册、事件/决定选择器、
两种架构编译和回退规则；连接外部 WinDbg/GDB 的端到端交互尚未列为自动通过项。

## 已完成：Python 配置高频插桩、原生执行

指令、内存、分支和插桩生命周期事件不能逐条同步进入 Python。脚本只声明要采集什么，原生层把声明编译
为不可变规则：

```python
generation = pb.instrumentation_set(
    kinds=[
        "instruction", "instruction.decode", "memory", "branch.edge",
        "trace.instrument", "routine.instrument", "basic_block.instrument",
    ],
    ranges=[(module_start, module_end), (jit_start, jit_end)],
    threads=[worker_tid],       # 省略或 [] 表示全部线程
)

current = pb.instrumentation_policy()
pb.instrumentation_clear()
```

规则属于当前插件；再次 `instrumentation_set` 是原子替换，不是追加。多个插件的规则在
原生层按逻辑“或”合并，但每个插件自己的 `种类 + 地址范围 + 线程` 仍保持同一组“且”关系，
不会错误地把 A 插件的线程和 B 插件的地址拼成放大的笛卡尔积。插件卸载、初始化失败或
回调异常进入 error 状态时，它的规则自动退出合并结果；清空最后一份 Python 规则后恢复
启动环境变量决定的默认引擎策略。

每个插件最多提交 64 个原始范围和 64 个线程号，所有运行插件合并后的范围上限同样是
64。相邻或重叠范围会先合并。更新时脚本线程发布新的不可变快照并让相关 Pin 代码缓存
失效；已经编译过的函数会重新进入插桩回调。插桩阶段按种类和范围决定是否插入分析调用，
运行阶段再次按种类、范围和线程过滤，因此旧代码缓存中的分析调用在规则收窄后立即失效。
Pin 热路径不获取 GIL、不调用 Python、不分配规则对象。

`pb.on("instruction"/"instruction.decode"/"memory"/"branch.edge", callback)` 决定 Python 如何消费已采集事件，
`pb.instrumentation_set` 决定原生层采集哪些事件；两者职责分开。真实 Pin 回归先在引擎
关闭时执行并缓存一个函数，再由 Python 只启用该函数范围，验证动态重新插桩成功且相邻
排除函数没有泄漏事件。

运行时 `instruction`、`memory`、`branch.edge` 事件的 `policy_generation` 取自完成地址/线程
匹配的同一份不可变原生策略快照，不是随后另读的全局计数器。`instruction` 同时提供 `size`
和按目标位数回绕的 `next_address`，脚本无需再从原始 `a0` 猜字段含义。

函数、Trace 和基本块生命周期沿用同一个规则快照，而不是另建一套容易交叉放宽的过滤器：

- `routine.instrument`：策略发布后遍历当前已加载镜像的 section/routine，补发范围内函数
  快照；以后 RTN 原生回调继续投递新函数；
- `trace.instrument`：Pin 创建动态 TRACE 时复制起点、大小、基本块数、指令数、是否贯穿
  以及所在函数地址；
- `basic_block.instrument`：在 TRACE 回调内遍历 BBL，复制起点、大小、指令数、贯穿和
  original 标志。

所有遍历都有镜像/节/函数数量硬上限，借用的 RTN/TRACE/BBL 句柄不离开回调。三类事件
使用普通遥测环和批量 Python 路由，允许覆盖并报告 `missed`，不把静态代码发现伪装成
同步控制流。它们没有应用线程，`tid=-1`；线程过滤仅作用于运行期 instruction/memory/
branch，地址范围仍适用于全部种类。策略变化失效范围内代码缓存，Trace/基本块可在同一
`policy_generation` 内重复出现，这是 Pin 重翻译的真实生命周期，不做虚假全局去重。

## 已完成：Python 配置内存地址转换、原生改写访存

地址转换不是逐次回调 Python。插件声明半开源区间、目标起点和选择器：

```python
generation = pb.memory_translation_set(
    [(virtual_start, virtual_end, backing_start)],
    threads=[worker_tid],                 # 省略或 [] 表示全部线程
    instruction_ranges=[(code_lo, code_hi)],
    operations=["load", "store"],
    include_pin=False,
)

policy = pb.memory_translation_policy()
pb.memory_translation_clear()
```

命中后保持区间内偏移：`translated = backing_start + (address - virtual_start)`。一次访问必须
完整位于源区间内，跨越边界的访问保持原地址。`instruction_ranges` 省略或空列表表示所有
应用指令；`operations` 可只选读取或写入；`include_pin=False` 默认不改写 Pin 自身访问。
原子读改写按写入分类。插件自己的映射不得重叠，所有运行插件之间也禁止源区间重叠；
冲突配置整体失败并恢复旧快照，避免脚本加载顺序暗中决定目标行为。

Pin 的全局内存转换回调用于让 Pin 工具侧取得一致地址，但它本身不会改变应用指令的真实
访存。因此 C ABI v1.7 增加固定原语 `pb_ins_insert_memory_address_translation`：插桩阶段
为最多两个普通内存操作数插入转换调用，把返回地址写入预先认领的两个工具寄存器，再用
`INS_RewriteMemoryOperand` 改写应用操作数。Python 策略变化时，相应指令范围的代码缓存
失效并重新 JIT；运行时只遍历不可变规则，不调用 Python、不分配、不加锁。

每个插件和所有运行插件合计最多 64 个映射；线程和指令范围各最多 64 项。插件卸载、
初始化失败或回调异常进入 error 状态时自动移除其映射。真实 Pin 回归把源变量映射到另一
个 backing 变量，证明指定函数的读取拿到 backing 值，同时未匹配的原子访问仍取得源值。

## 已完成：Python 预置机器码、原生快速取码

取码也不逐次进入 Python。插件一次性提交虚拟地址和字节段：

```python
generation = pb.code_fetch_set([
    (function_address, replacement_bytes),
])

policy = pb.code_fetch_policy()
pb.code_fetch_clear()
```

所有运行插件的字节段合并成一个按地址排序的不可变快照。Pin 取码请求命中预置段时直接
复制字节；一个请求同时覆盖预置段和普通地址时，分段处理，普通部分通过原生安全取码
回退并填写 Pin 提供的异常对象。回调只做原子读、二分查找和内存复制，不分配、不加锁、
不发 RPC、不取得 GIL。策略发布在脚本线程完成，并使旧、新字节段覆盖范围的 Pin 翻译
失效，已经执行过的函数也会重新取码。

每个插件和所有运行插件合计最多 64 段、1 MiB；空段、地址溢出和任意插件之间的重叠段
均被拒绝，失败时恢复上一份策略。插件卸载、初始化失败或回调异常进入 error 状态时自动
移除它的字节段。C ABI v1.8 增加 `pb_pin_fetch_original_code`，专供取码回调读取原应用
字节；它直接使用 `PIN_SafeCopyEx`，不会像 `PIN_FetchCode` 那样递归进入已注册取码器。

取码器在第一份非空策略发布时才动态注册，并持有 Pin client lock；没使用该功能的进程
不改变取码语义。Pin 不提供撤销取码器的接口，因此第一次启用后，`clear` 会切换为全量
原始安全取码，但注册本身保持到进程结束。Intel Pin 明确规定：工具使用自定义取码器后，
Pin 不再自动负责检测全部自修改代码。平台会对 `code_fetch_set/clear` 涉及的范围主动失效，
但目标进程在这些 API 之外自行修改代码时，脚本必须显式重新发布相应段；不能再依赖自动
SMC 检测覆盖全部情况。

真实 Pin 回归先执行并缓存返回 `1` 的函数，完成握手后由 Python 读取另一函数的机器码并
映射到原函数，第二次调用返回 `2`，同时验证未映射地址仍能正常取码。

## 已完成：XED 解码输入与已解码指令通知

Pin 的 `PIN_AddXedDecodeCallbackFunction` 在 XED 解码之前运行，没有指令地址，也不是
解码结果通知。平台没有把这个借用指针伪装成异步 Python 事件，而是拆成两条职责清楚的
链路：

```python
pb.xed_decode_set(cldemote=True, cet=None, mpx=None)
pb.on("instruction.decode", on_decoded)
pb.instrumentation_set(
    kinds=["instruction.decode"],
    ranges=[(code_start, code_end)],
)
```

第一条链路把每个插件声明的 CET、CLDEMOTE、MPX 布尔输入合并为进程级原子快照。解码
线程只执行一次原子读取和固定 C ABI 调用，不进入 Python、不分配、不加锁。不同插件对
同一项给出相反明确值时拒绝更新；更新成功后全局失效旧翻译，使已 JIT 代码按新输入重新
解码。C ABI v1.9 增加 `pb_xed_decoded_inst_set_features`，只在 Pin 回调有效期内修改受支持
字段。

第二条链路在普通 INS 插桩回调中运行，此时 Pin 已经完成解码且有稳定地址。原生层先应用
`instrumentation_set` 的种类和地址范围，再复制长度、XED 类别、扩展、操作码、内存操作数
数量、控制流标志、直接目标和策略版本复制到固定事件记录。`pb.on("instruction.decode")`
可逐条处理，`pb.watch(["instruction.decode"])` + `on_event_batch` 可批量处理；借用的 INS/XED
句柄从不离开 Pin 回调。该事件是静态插桩事件，`thread_id == -1`，线程过滤只适用于运行时事件。

真实 Pin 回归使用 Pin 官方测试相同的 `0F 1C 00` CLDEMOTE 编码，验证 Python 策略让 XED
识别为 CLDEMOTE、原生地址范围没有泄漏，并同时通过命名回调和批处理回调收到复制结果。

### 当前交付状态

| 功能 | Python 入口 | 当前状态 |
|---|---|---|
| 同名脚本安全更新 | `script run` | 先编译并私下暂存初始化；暂存版不进入派发或原生策略快照，语法/运行期初始化错误及策略冲突均恢复旧插件，整组策略校验成功后才调用旧版 `on_unload` 并提交，真实 Pin 决策插件回归通过 |
| x86 Python 控制面 | 与 x64 相同的 `pb.*` | PyO3 交叉构建、x86 CPython 部署和 COFF DATA 导入已完成；真实 ia32 Pin 下 Python 读取 EIP、按 32 位 XED 模式解码 `40 C3`、注册精确断点并从回调恢复目标通过 |
| 失败插件资源隔离 | 所有 Python 执行入口 | 错误状态保留；断点、重复 Hook 租约和原生策略统一撤销，真实 Pin 注册后故障测试验证 `hooks=0`、`bps=0` |
| 初始化期间事件边界 | `pb.on(...)` / `pb.breakpoint(...)` | 插件级游标在执行前建立，命名处理函数按通道记录精确注册边界；真实 Hook 初始化竞态测试通过 |
| 精确断点处理 | `pb.breakpoint` | 已完成，真实 Pin 测试通过 |
| 单步落点隔离 | 回调返回 `step_into/step_over` 或 `pb.step` | 单步使用独立后继槽并严格匹配线程；不会占用、删除或提前完成普通断点，容量/线程/重放规则有 Rust 单测 |
| 执行区间精确停机 | `pb.execution_trap` + `pb.on("execution.trap", ...)` | 已完成；原生热路径匹配、插件所有权清理和稳定停止后投递已接入，通用真实 Pin fixture 通过 |
| 进程/线程生命周期 | `pb.on(...)` | 已完成，真实 Pin 测试通过 |
| 退出前 Python 清理 | `pb.on("process.exit/process.prepare_fini", ...)` | 已完成，两阶段顺序派发和原生 PrepareForFini 到达均经真实 Pin 验证 |
| 最终退出记录 | 无 Python 回调；原生 Fini 事件和总结日志 | 已完成；该阶段 Pin 已停止脚本调度，不伪装成可执行 Python 回调 |
| 动态机器码修改 | `pb.on("code.smc", ...)` | 已完成，真实 Pin 测试通过 |
| 模块加载/卸载 | `pb.on("module.load/module.unload", ...)` | 高优先级/兼容环双写和逐处理函数去重完成，真实 DLL 加载/卸载测试各精确一次 |
| 内存不足 | `pb.on("memory.oom", ...)` | 原生紧急日志、原子保底通知和去重完成；单元/契约测试通过，无法安全强制触发真实耗尽 |
| Pin 内部异常 | `pb.on("pin.internal_exception", ...)` | 原生崩溃记录后投递高优先级快照；仅在进程存活时可到达 Python |
| Pin 分离完成 | `pb.on("pin.detach", ...)` | JIT/Probe 原生接入完成；分离后的即时 Python 调度不承诺 |
| Pin 重新附加 | `pb.pin_attach_supported/pin_detach/pin_attach` + `pb.on("pin.attach", ...)` | ABI 和统一回调重建链已完成；Windows JIT 由 Pin 3.31 明确不支持并已验证安全拒绝，支持平台的完整往返待验证 |
| 子进程跟随决策 | `pb.intercept("child.follow", ...)` | 已完成；跟随/不跟随、独立子端口、子日志隔离和经子控制面热加载 Python 均通过真实 Pin 测试 |
| Hook 异步观察 | `pb.on("hook.entry/return", ..., address=...)` | 自动挂载/地址过滤、独立观察环和同步/异步共享租约完成，真实 Pin 的 once/常驻及双向释放测试通过 |
| Hook 同步决定 | `pb.intercept("hook.entry/return", ..., address=...)` | 已完成，入口跳过/返回值改写真实 Pin 测试通过 |
| 系统调用观察 | `pb.on("syscall", ..., numbers=...)` / `on_syscall` | 独立原生过滤观察环、兼容双写和逐处理函数去重完成，真实 Pin 测试通过且丢失为 0 |
| 系统调用同步决定 | `pb.intercept("syscall.entry/exit", ..., numbers=...)` | 已完成，入口参数/出口返回值真实 Pin 测试通过 |
| 上下文/异常观察 | `pb.on("context.change/exception", ...)` / `on_exception` | 六类原因高优先级/兼容双写、乱序去重和前后 IP schema 完成；真实 APC 与异常测试通过 |
| 异常同步决定 | `pb.intercept("exception.handle", ..., codes=...)` | 已完成，异常现场读取/目标上下文改写真实 Pin 测试通过 |
| 调试器事件观察 | `pb.on("debugger.breakpoint/single_step/async_break", ...)` | 已完成，三类 Pin 回调进入高优先级队列；附加调试器交互测试待独立环境 |
| 调试器同步决定 | `pb.intercept("debugger.breakpoint/single_step/async_break", ...)` | 已完成，支持去向决定和寄存器改写，强制执行 Pin 的异步中断/IP 限制；ABI 与 Rust 测试通过 |
| 指令/内存/分支插桩规则 | `pb.instrumentation_set/clear/policy` | 已完成，动态重新插桩和原生范围过滤真实 Pin 测试通过 |
| 地址转换 | `pb.memory_translation_set/clear/policy` | 已完成，真实访存改写和原生选择器真实 Pin 测试通过 |
| 取机器码 | `pb.code_fetch_set/clear/policy` | 已完成，动态重新取码和原始地址回退真实 Pin 测试通过 |
| XED 解码输入 | `pb.xed_decode_set/clear/policy` | 已完成，CET/CLDEMOTE/MPX 原生预解码配置，冲突回滚和重新解码已接入 |
| 已解码指令通知 | `pb.on("instruction.decode")` + `pb.instrumentation_set` | 已完成，原生范围过滤、命名/批量 Python 回调真实 Pin 测试通过 |
| 函数/Trace/基本块生命周期 | `pb.on(...)` + `pb.instrumentation_set` | 已完成，热加载函数快照、动态重新翻译、三类原生范围过滤真实 Pin 测试通过 |
