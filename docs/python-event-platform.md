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

异常、线程生命周期、模块生命周期、动态代码修改、进程生命周期等低频事件进入独立的高优先级队列。原生回调只复制固定大小的数据，Python 随后处理，不在 Pin 回调线程等待 Python。

### 普通遥测

指令执行、内存访问、分支、解码和插桩元数据属于高频遥测。它们必须经过原生过滤并批量交给 Python。队列满时记录明确的丢失计数，不能阻塞目标线程。

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
- 相同地址在底层复用一个断点编号；
- 只有不存在其他新式订阅时才移除底层断点；
- 旧 `pb.bp_set` / `pb.bp_remove` 保持原有全局语义，不与新接口混用所有权。

## 兼容接口

以下接口继续保留：

- `pb.bp_set`、`pb.bp_remove`；
- 顶层 `on_bp_hit(event)`；
- 顶层 `on_stop(tid, address)`。

旧回调收到的字段和停止行为不改变。新式断点处理完成后，旧 `on_bp_hit` 和 `on_stop` 仍能收到通知；如果新式处理已经恢复目标，宿主不得再次恢复或单步。

## 模块边界

- `scripting/subscriptions.rs`：订阅数据、断点所有权和动作类型；
- `scripting/api.rs`：Python 参数校验和注册入口；
- `scripting/host.rs`：取得停止快照、调用处理函数、合并并执行最终决定；
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

应用启动是粘性状态：脚本通常在目标入口停住后才加载，因此 `process.start` 会对每个
新订阅补发一次。`process.exit` 在 `RtlExitUserProcess`/`ExitProcess` 入口提前投递，
`PrepareForFini` 处理绕过常规退出 API 的路径；原生层等待脚本线程确认，默认最多
1000ms，超时后继续退出。真正的 `Fini` 仍写入原生事件和总结日志，不虚假承诺 Python
能在最终销毁阶段运行。

线程和应用回调遵守同一个硬边界：Pin 回调只读取上下文指令地址并提交 POD 事件，绝不
直接调用 Python。系统调用命名订阅会参加原生开关合并，没有脚本订阅时不为接口表象
而额外开启事件流。

## 已完成：高优先级事件通道

第三阶段增加了独立于 64K 遥测环的 4096 槽高优先级环。进程/线程生命周期、
`code.smc`、`pin.detach`、`pin.attach` 和 `memory.oom` 使用自己的游标，脚本宿主每个
节拍先处理高优先级记录，再处理断点和普通遥测。因此指令或内存事件洪流不会再把这些
记录从普通环中挤掉。

高优先级原生生产者仍然不能直接调用 Python，也不能等待普通互斥锁：它们只做固定大小
记录和 Pin mutex 的 try-lock。锁竞争导致的丢失计入 `priority_dropped`，在 Fini 总结
日志中可见；环覆盖造成的游标缺口计入插件的 `dropped`。

`code.smc` 在第一个 Python 订阅出现时才注册，因为 Pin 会从注册后开始维护 SMC 跟踪
状态，长期全局开启可能无限增长。运行中注册时由宿主持有 Pin client lock。真实 Pin
测试会执行、修改并再次执行一段动态机器码，验证 Python 收到 `trace_start`/
`trace_end`。

JIT 和 Probe 的分离完成回调在 C ABI v1.6 中严格分开：
`pb_pin_add_detach_function` 对应 JIT，`pb_pin_add_detach_function_probed` 对应 Probe；
C 后端和 agent 都按当前模式选择，避免把 Probe 回调注册到 JIT 启动路径。

`pin.attach` 的公开事件结构已经固定，但完整的“Python 发起分离后持续驻留、重新附加、
重建内部线程和回调”尚未完成，不能把第二次 application-start 的检测代码等同于已经
交付了整套重新附加控制链。`memory.oom` 也只能用契约测试覆盖注册和字段结构，真实耗尽
目标内存不是常规回归测试的安全做法。

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
以及保留原始数据的 `argv_bytes`。处理函数必须返回 `bool` 或
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
`child_decisions`、`child_follow`、`child_reject` 和 `child_decision_timeouts`。当前被跟随的
子进程会继承父进程固定的 `PINBRIDGE_AGENT_PORT`，因此子 agent 无法绑定同一端口，会继续
插桩但没有自己的 Python/查询控制面；每个子进程的独立端口和命令行改写仍是后续工作。

### Hook 同步拦截

Hook 的异步观察继续使用 `pb.on("hook.entry", ...)` / `pb.on("hook.return", ...)`；需要
在原指令执行前取得 Python 返回值时使用：

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
目标 RPC 快速失败。插件卸载、替换、`once` 完成或 `pb.unintercept` 会释放该插件的
所有权；若注册前 Hook 点不存在，并且全部脚本订阅都已释放，则延迟移除该原生点。

真实 Pin 测试同时覆盖入口直接返回（证明原函数未执行）和返回指令改写返回值；Fini
日志使用 `sync_decisions`、`sync_timeouts`、`sync_busy` 证明原生采用了两次 Python 决定。

### 当前交付状态

| 功能 | Python 入口 | 当前状态 |
|---|---|---|
| 精确断点处理 | `pb.breakpoint` | 已完成，真实 Pin 测试通过 |
| 进程/线程生命周期 | `pb.on(...)` | 已完成，真实 Pin 测试通过 |
| 动态机器码修改 | `pb.on("code.smc", ...)` | 已完成，真实 Pin 测试通过 |
| 内存不足 | `pb.on("memory.oom", ...)` | 原生接入和契约测试完成，无法安全强制触发 |
| Pin 分离完成 | `pb.on("pin.detach", ...)` | JIT/Probe 原生接入完成；分离后的即时 Python 调度不承诺 |
| Pin 重新附加 | `pb.on("pin.attach", ...)` | 事件结构已完成，重新附加控制链待开发 |
| 子进程跟随决策 | `pb.intercept("child.follow", ...)` | 已完成，跟随/不跟随真实 Pin 测试通过；子进程独立控制端口待开发 |
| Hook 同步决定 | `pb.intercept("hook.entry/return", ..., address=...)` | 已完成，入口跳过/返回值改写真实 Pin 测试通过 |
| 系统调用/异常同步决定 | 尚未发布 | 待开发；现有命名事件仅观察 |
| 地址转换/取码/插桩规则 | 尚未发布 | 待开发；必须采用 Python 配置、原生执行 |
