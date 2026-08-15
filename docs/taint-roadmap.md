# 污点分析路线图

目标：在 PinBridge 之上做脱壳/逆向导向的污点分析。分三层，自底向上，逐层交付。

## 1. 编排层（已完成）

agent 内嵌 CPython 多插件脚本宿主（见 `docs/scripting.md`)：事件订阅、断点/hook、内存与
寄存器读写、停走单步、符号解析——全部分析逻辑的编写与调试都在这一层完成，是后两层的
控制面和验证场。

## 2. 录制通道（已交付，ABI v1.5)

把"跑过的那段执行"无损落到盘上：

- `trace start/stop` 控制操作（op 0x35/0x36/0x37;CLI `trace start <kinds> <lo> <hi> <path>` /
  `trace stop` / `tracest`;pb API `pb.trace_start/trace_stop/trace_status`)；独立大容量
  slab(`PINBRIDGE_AGENT_RECORD_CAP`，默认 1M 槽 × 96B);Pin 内部线程 drain 落盘。
- **采集域规格**（op 0x38；CLI `trace start-spec <kinds> <lo-hi[,lo-hi]> <threads|all> <path>`；
  Python `pb.trace_start_spec`）：在 native recorder 的 ring claim 之前同时按多个地址范围和
  线程白名单过滤。空线程列表表示全部线程，最多 16 个范围和 64 个线程；PBTR 元数据保留
  `modules`、`ranges`、`threads`，因此脚本可以证明“哪个模块、哪些地址范围、哪些线程被录制”。
  旧单范围 API 保持兼容。
- **在线范围策略**（op 0x39；`pb.trace_extend`）：脚本可在录制进行中查询
  `pb.memory_region`，识别 `MEM_PRIVATE + PAGE_EXECUTE*` 的临时代码区并原子追加范围；
  PBTR 用 marker 保存扩展边界，避免把整个进程放进采集域。
- **档 2 抓取**（在现有"抓地址"档位之上加"抓内容"档位）:
  - exec 事件携带**指令字节**(kind 9，经 `pb_ins_insert_capture_exec_bytes`);
  - memory 事件携带**实际读/写的值**(kind 10，经 `pb_ins_insert_memory_operands_values`;
    写操作数在 IPOINT_AFTER 落值，无 fall-through 时退化为写前内容）;
  - `registers`/`reg_snapshot`（kind 13）可选地在每条指令前记录 GP、RIP、RFLAGS
    和 XMM/YMM/ZMM0-31；同一指令的组件通过 `arg7=frame_id` 组合。该通道默认关闭，
    避免未请求的上下文开销；首帧为基线，后续帧使用变化掩码和分片值。
- 录制面向单线程窗口：圈定 OEP 候选段、解密循环、API 解混淆序列，开录→跑过→停录。
- 文件格式（PBTR v1）见 `record.rs` 头部注释：固定 16 字节头 + meta JSON + 88 字节
  EventRecord 记录流；kind 5/6 可选记录 syscall 和 exception/context-change，kind 11 为录制器标注（start/stop marker)，kind 12 为无损的
  连续重复压缩标记（`arg0` 是额外重复次数，`sequence` 是该段最后序号，`arg1` 保留原 kind）；kind 13 为寄存器上下文分量。
  重放端默认展开 kind 12，AI/前端检查可用不展开的流式模式；读者跳过未知 kind 并容忍截断尾部。
  `pb_rpc_fixture --spin` 提供主模块热窗口用于验证。

## 3. 重放分析层（规划中）

纯 Python 的 `taint_replay` 库，对录制下来的轨迹做离线分析：**前向污点传播**（从输入/
密钥/密文出发看数据流向）与**后向切片**（从某个寄存器/内存值倒推它的生产者链）。

为什么重放可行：

- **具体 EA 消灭别名问题**——每条 memory 事件记录的是确定地址与确定值，不做指针分析，
  不靠猜测；
- **录制值 + 入口上下文 = 确定性重放**——单线程窗口内，内存读的答案都在带上，从入口
  快照出发逐条 exec 重执行，结果与当时一致。

边界（先说清楚，不越界）:

- **不做 concolic 引导**：重放只沿录制到的那条路径走，不探索分支另一侧；
- **rdtsc 类指令特判**（读时戳/随机源等"每次执行结果不同"的指令按录制值回填或打标）;
- **窗口内必须无损**：录制 ring 溢出=该窗口作废，重放拒绝在带洞的轨迹上运行（宁可
  重录，不出错结果）。
