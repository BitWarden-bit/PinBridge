# PinBridge UI 工作台交接（2026-08-18）

## Hook 能力落地更新（本节优先于下方旧交接说明）

Hook 已按两个真实工作流拆分为「指令 Hook」与「签名驱动的函数调用日志」，传统断点仍完全
独立，不要求每个 Hook 都携带同步回调。一键 Hook 的真实语义是记录函数调用：入口线程、按
函数原型解析的参数类型/大小/值和声明返回类型；它不是同步接管，也不会替换传统指令 Hook。

- UI 清单使用后端 `kind=instruction/api` 硬过滤：普通指令点只出现在「指令 Hook」，函数调用
  点只出现在「API Hook」。API 导出表不再显示“仅指令 Hook”；同一导出地址上只有普通指令点
  时，API 状态仍是“未记录 API Hook”。统一日志的类型字段只信任 Agent 原生类型位，不根据
  符号名或已有签名猜测分类。

- PE 导出表只有名称和地址，没有 C/C++ 原型。类型签名必须标注 `pdb`、`header`、`manual` 或
  `ai_inferred` 来源及 0–100 置信度；没有签名时 UI/MCP 明确显示“原始 ABI”，禁止把寄存器
  值猜成 `int`、指针或字符串。AI 反推结果永远不会伪装成 PDB/头文件的权威签名。
- `hook_function_set` 现在强制接收 C 风格 `signature`、`signature_source` 和
  `signature_confidence`；`hook_signature_set/remove` 可独立更新已有点，清单固定显示原型、
  来源、置信度、调用约定、参数名/类型/字节大小和返回类型。

- 原生 Hook 容量从 4096 提升到 32768，规则容量为 65536；DLL 命名导出上限同步提升到
  32768，兼容至少 2 万个唯一地址。
- 单函数使用 `hook_function_set`，DLL 全部导出经 Client/Hub/MCP/Tauri/UI 使用单次
  `HOOK_SET_BATCH` 发布；地址排序去重、与现有快照线性合并，并按 64 KiB 代码窗口合并 JIT
  失效区间，不再逐导出重建和失效。已有指令 Hook 可原址升级为函数调用日志。
- Agent 保存紧凑的无文本 ABI 布局，入口最多采集 16 个固定参数。x64 按签名在
  RCX/RDX/R8/R9、XMM0–3 与后续栈槽间选择；x86 按声明的 cdecl/stdcall/fastcall 位置读取。
  整数返回读 RAX/EAX，`float/double` 返回读 XMM0。每线程固定调用栈使用入口栈指针配对
  `ret`，避免无符号模块中相邻函数共用 RTN 时产生伪返回；同步跳过函数会取消返回等待。
- 命中筛选分三层：JIT 插桩只给地址快照二分命中的指令插分析回调；同步原生入口按
  `(address, entry/return, thread_id/全线程)` 最多二分两次；脚本线程通过同键哈希桶只取
  当前命中的回调 ID，不遍历插件或全部 decision。异步观察也按地址与入口/返回精确过滤。
- `pb.intercept("hook.entry/return")` 强制携带 1–512 字节单行 `description`。清单和 UI 详情
  显示创建者、回调函数、代码、说明、明确的分发筛选器、最近返回/错误与脚本输出。
- UI 新增固定的「统一日志」一级页签，不再把命中结果藏在单个 Hook 详情中。Agent 在真实命中
  时写入 UTC 纳秒时间和不可混淆的 `api/instruction` 类型位；时间线可按时间正倒序、筛选
  API Hook/普通指令 Hook、入口/返回、模块/函数/地址/线程，并按线程+函数配对 API 入口返回。
  当前页可继续向前翻完整的 32768 条保留窗口，单点详情仍保留最近命中。
- UI 的全量 Hook 清单不参与 1.6 秒轮询；统一日志使用独立 32768 槽有界通道，每条记录
  可带 16 个签名解析后的 ABI 值；UI/MCP 每页最多读取 4096 条，不扫描也不依赖通用遥测环，因此 instruction/memory 洪流不会挤掉
  参数和返回值。接口分别报告生产竞争丢弃 `lane_dropped` 与历史覆盖
  `history_overwritten`。清单仅在进入、停止代次、人工刷新或修改后更新，指令清单每页 300、
  导出表每页 400，避免 2 万点时重复符号解析或创建上万个 DOM 节点。
- Hub inventory 先按地址一次性建立回调索引，再线性合并 Hook 与符号结果，不做
  `Hook 数 × 回调数` 扫描；符号解析按 Agent 支持的 4096 地址满批请求。认证 IPC 响应帧上限
  提高到 16 MiB 以容纳 2 万点详细清单，请求仍限制为 2 MiB。
- `hook_function_set`、`hook_signature_set/remove`、`hook_inventory/monitor/module` 与
  `module_exports` 已贯通 Hub、MCP
  与桌面命令；Agent 协议用 `HOOK_FUNCTION_LIST` 区分函数日志与普通指令点，并通过
  `HOOK_EVENTS_NEWEST` 按 `before` 序列游标分页读取专用日志。DLL/API 符号只在首次出现时解析
  并缓存，2 万点轮询不会重复扫描或复制整份符号表。删除存在同步回调的物理 Hook 会被拒绝，必须先卸载
  对应脚本，防止破坏共享所有权。

## 异常能力落地更新（本节优先于下方旧交接说明）

异常模块已经按用户要求严格分成两个一级视图：**监控**与**操作**，不是只读异常日志。

- Agent 新增独立高优先级查询通道，目标异常与 Pin / Agent 内部异常不会再被高频执行事件挤掉；接口同时返回通道累计量与真实丢弃量。
- 每个目标异常用同一 `context_generation` 串联两条原生记录：异常边沿记录系统准备采用的目标上下文；处置记录在所有 `exception.handle` 回调结束后再次读取上下文，保存最终 IP、是否运行回调、GP 寄存器修改掩码。
- Hub/MCP/Tauri 暴露 `exception_monitor`、`exception_policy_get/set`、`exception_inventory`。监控结果直接给出“异常现场 → 系统分发目标 → 最终执行去向”，以及 `system / inspected / takeover` 真实状态。
- 「监控」可筛选目标异常与 Pin / Agent 内部异常；详情能定位三个地址、查看修改寄存器，并从具体异常一键进入接管编辑器。
- 「操作」包含真实异常暂停策略和 `pb.intercept("exception.handle")` 清单。回调详情显示创建者、过滤条件、最近代次、最近返回/错误、脚本输出和已知源码。
- 新建接管模板明确区分 `from_registers`（异常现场）与 `registers`（系统准备采用的上下文）：返回 `None` 保持系统路径；返回 `{"registers": {...}}` 改写最终上下文。模板默认不接管，必须填写真实恢复地址；x64 直接跳入函数时同时提示 ABI/栈责任。
- `EVENT_EXCEPTION_DISPOSITION = 33` 只写入专用高优先级通道，不混入普通脚本事件；旧的传统调试、断点、单步语义保持不变。
- 真机演示使用 `fixtures/exception_ui_demo/exception_ui_demo_x64_v3.exe`。人工脚本把访问冲突的 `rip/rsp` 改到导出的 `RecoveryPoint`，UI 已验证显示回调接管、修改 `rsp, rip`、最终落点为恢复入口；暂停策略随后真实停住目标。

验证：Agent 40、Client 20、Hub 27、MCP 10、UI 9 项测试通过；Rust `cargo check --locked`、前端 `npm run build`、桌面端 Release 构建通过。当前新版 UI 与真实异常接管目标保持打开。

### 同步接管反汇编补充

`pb.disasm(addr, count<=128)` 已从 loopback RPC 改为 Agent 进程内的
`PIN_SafeCopy + XED` 解码，所以 `exception.handle`、`hook.entry`、`hook.return` 等
`pb.intercept(...)` 同步回调内可直接查看现场汇编。返回格式保持
`[(address, size, kind, target, text), ...]`。真实异常回归在访问冲突回调中解出
`mov dword ptr [0x1], 0x42` 后改写 `rip/rsp` 恢复；真实 Hook 回归在入口同步回调中解出
`lock inc dword ptr [rip+0x20af9]`，3 次同步决定全部生效且无超时。

## 断点管理落地更新（本节优先于下方旧交接说明）

用户已明确批准本轮同时修改 Agent、Client、Hub、MCP、Tauri 与 UI。当前实现只暴露后端已经存在的能力，不再使用假数据或预设的“分析摘要 / 进度 DSL / 任意结构化回调结果”等概念。

- 普通断点仍保持传统调试器语义，回调是可选绑定；列表区分 `traditional`、`callback`、`mixed`、`external`。
- Hub 记录通过 Hub 下断的来源（`human` / `ai`），Agent 列出真实脚本绑定：插件名、回调函数名、`once`、线程过滤、最近停止代次、最近实际动作和异常。
- 回调只接受现有后端动作：`stay`、`resume`、`step_into`、`step_over`。UI 与 MCP 显示的是实际解析结果或实际异常，不虚构任意返回正文。
- 回调挂到已有普通断点时，普通断点的停住语义优先；之后再添加普通断点也会把共享原生断点标记为传统所有者。
- Hub 保存经当前 Hub 注入/替换的完整脚本源码及创建者、最后修改者；启动目录自动加载、或 Hub 重启前已存在的脚本只能显示绑定元数据，源码明确标记为不可用。
- MCP 新增 `breakpoint_inventory` 与 `script_get`；UI 详情可查看/编辑已知源码，也可从普通断点创建一个真实的 `pb.breakpoint(...)` 插件。
- 每个 `pb.breakpoint` 回调必须携带 1–512 字节的单行 `description`。AI/MCP 入口要求在每个调用点写非空字符串字面量，Agent 再做最终校验；清单和 UI 详情固定显示这段说明。
- 回调最近一次完整 Python 返回值以 4 KiB 有界 `repr` 保存为 `last_return`；`last_action` 仍是后端解析后的停止动作，`last_error` 独立记录异常。MCP inventory 和 UI 详情同时展示三者。
- 当前原生删除接口会删除共享物理断点，因此 Hub/UI 在存在回调绑定时拒绝直接物理删除，要求先处理所属脚本，避免误删传统或其他脚本共享的断点。

验证：`cargo check --locked`（Agent/Client/Hub/MCP/UI）通过；Agent 40、Client 20、Hub 27、MCP 10、UI 9 项测试通过；`npm run build` 通过。原先的 `pb_pin_enable_single_step_passthrough` 链接问题已通过使用当前源码重建 x64 Debug `pinbridge.lib` 解决。

## 一句话状态

右侧自动化面已从旧原型清空重建：**断点管理**、**AI 活动**两个面板接入真实后端数据；全应用换成中性石墨主题（无蓝色）；启动页、左右分栏壳、左侧调试器全部真实可用。**所有改动未提交 git**（main 分支，基于 126f5c5）。

## 接手第一件事

AI 活动面板写完时 UI 窗口开着，`pinbridge-ui.exe` 没能重新链接。接手后：

```powershell
# 1. 确认没有 pinbridge-ui.exe 在运行（开着会 os error 5 拒绝访问）
# 2. 打包（前端 dist 已是新的，不用再 npm build）
cargo build --release --locked --manifest-path bindings/rust/Cargo.toml -p pinbridge-ui
# 3. 运行（stdout 必须重定向，目标程序的 console 输出会灌爆管道）
cd bindings/rust/target/release
./pinbridge-ui.exe > ../../../../build/ui-run.log 2>&1
```

启动目标断下后，右侧「AI 活动」页签应显示真实操作时间线（人工的下断/单步等操作也会记录）。

## 验证状态

| 项 | 状态 |
| --- | --- |
| `npm run build`（vite） | 通过（含 AI 活动面板） |
| `cargo build -p pinbridge-ui --release` | 主题版本已打包运行过；含 AI 活动面板的版本**未打包未真机验证** |
| `cargo test -p pinbridge-ui` | 未受影响（9 个测试，本轮没动 Rust） |
| RFLAGS 标志位解析 | 位运算经 node 验证（0x202 → 仅 IF 置位；ZF 取反 → 0x242）；真机 UI 待用户确认 |
| 断点管理面板 | 用户在真机会话中看过界面；交互（下断/删除/定位）待逐条确认 |
| 中性主题 | 用户看过启动页截图并继续推进，方向认可；细节未逐条走查 |

## 用户硬性设计要求（务必遵守）

- **旧 pbw 原型已整体删除，不许复用**（功能菜单主页、回调工作区、板卡、五代 CSS：蓝色工作区/metro/玻璃/ambient 全部否决）。
- **不要蓝色**，"UI 感不要太重"。当前主题：石墨灰阶（`--bg0 #0d0d0e` → `--bg4 #25272c`），颜色只表达状态语义：
  - 绿 `--ok #6fbf8f` = 已停止/已连接；红 `--danger #d47a85` = 错误；琥珀 `--warn #c9a35f` = 警告
  - AI =  muted 紫 `--ai #a78bca`（横幅、跟随开关、反汇编槽点、状态栏、活动时间线统一）
  - 断点归属：人工 = 白点，策略 = 红点，AI = 紫点（反汇编槽点与右侧列表一致）
  - 主按钮 = 白底黑字高对比；焦点 = 灰描边不发光；圆角 4–6px
- **只做 UI，不改后端**。`main.rs` / `hub-core` 不许动，除非用户明确批准（脚本面板需要新增 `cmd_script_*` 透传命令，提出后被用户叫停过一次）。
- **一切真实数据**，禁止模拟/假数据。旧设计的 hash 深链预览模式和全部硬编码 VMP 场景数据（PLAN_STAGES/ASSETS/CODE/AI_OPS 等）已删除。
- **FeaturePanel 模板**：每个自动化功能 = 头部（标题 + 唯一主操作按钮）+ 完整管理列表 + 底部一行说明。主操作必须是一个简单按钮（参考断点面板的「＋ 在当前地址下断」），完整能力收在列表里。后续功能套同一个壳，不要再设计新布局。

## 代码地图

- `ui/src/App.jsx` — 入口直接渲染 `UnifiedWorkbench`；旧应用完整保留为 `LegacyDebuggerApp`（无路由可达，集成备份，勿删）。
- `ui/src/features/workspace/UnifiedWorkbench.jsx` — 工作台壳：顶栏（目标/停止状态/控制权/跟随 AI/暂停继续）、左右分栏、状态栏；快照订阅、停止代次刷新、Hub 活动 3.5s 轮询（`refreshHub`）、AI live-op 推导（`activityToAiOp`）。
- `ui/src/features/workspace/LaunchWorkspace.jsx` — 启动页：目标/参数/Pin 路径、JIT vs 原生观察单选卡、入口断点、最近会话（localStorage）、loopback attach、环境自检。
- `ui/src/features/workspace/AutomationPane.jsx` — 右侧自动化面：功能页签（断点✅、AI 活动✅，异常/Hook/Trace/脚本禁用占位）、`FeaturePanel` 模板、断点面板、活动时间线面板。
- `ui/src/components/` — 复用的调试器组件：`Toolbar`、`DisasmView`（断点归属槽点 + AI 触碰高亮 + 跟随滚动）、`Registers`（含 RFLAGS/EFLAGS 标志位行，点击取反写回）、`BottomTabs`（`tabs` prop 控制页签，工作台只用 mem+stack）、`LaunchScreen`/`StatsPanel`（仅 legacy 用）。
- `ui/src/store.js` — Agent 4Hz 快照（Tauri event）：连接态/停止态/stopGen/命中 tid+addr/断点列表/事件表。
- `ui/src/api.js` — `invoke` 封装：`call`（错误走 pb-error 事件）与 `callWithError`（返回错误文本，面板内联报错用）。
- `ui/src/style.css` — 单文件主题，结构：基础变量与组件 → legacy AI desk → 工作台壳（pbw-*）→ 自动化面板（pba-*）→ 启动页。
- `src/main.rs` — Tauri 命令面（本轮未改）。

## 前端可用的真实接口（main.rs 已暴露）

断点 set/remove/list、寄存器 get/set、内存读写、反汇编（含向上对齐）、模块、线程、地址解析、`control_status / control_handoff_to_ai / control_takeover_manual`、`session_status`、`activity_list / activity_get`。

**Hub 已有但 UI 未暴露**（接脚本面板时需要 `main.rs` 透传 + `invoke_handler` 注册，先取得用户同意）：
`script_inject / script_replace`（args: `name`, `source`；返回 `script_id/generation/source_hash/state`）、`script_remove`（`name`）、`script_list`、`script_status`、`script_output`（`cursor/limit`，输出行带 `plugin` 字段）。

## 运维注意

- 打包前必须关闭 UI 窗口（exe 文件锁）；用户习惯自己关，问一声即可。
- 后台运行 UI 务必 `> build/ui-run.log 2>&1` 重定向（目标 console 菜单循环曾产生 16MiB 输出把进程杀掉）。
- 截图检查：`powershell -ExecutionPolicy Bypass -File tests/Shot-Ui.ps1` → `build/ui_shot.png`。
- 目标读取控制台输入时按 README 建议用桌面启动页或第二终端 attach，避免 stdin 竞争。
- `UnifiedWorkbench.jsx` / `style.css` 已统一为 LF；仓库 core.autocrlf 会提示 LF→CRLF，属正常。

## 待办（建议顺序）

1. 打包验证 AI 活动面板（步骤见文首）。
2. 主题细节走查：对比度、间距、字号，逐条与用户确认。
3. 异常 / Hook / Trace 面板：后端无独立规则 API，合理形态是"脚本模板生成 pb 代码"，与脚本面板一起设计。
4. 脚本面板：先问用户是否允许在 `main.rs` 加透传命令（接口格式上文已查明）。
5. 观察模式（probe）能力降级映射到 UI 禁用态。
6. 断点归属持久化（人工/AI/策略）与冲突提示。
