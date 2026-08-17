import React, { useMemo, useState } from "react";
import Toolbar from "../../components/Toolbar";
import DisasmView from "../../components/DisasmView";
import Registers from "../../components/Registers";
import StatsPanel from "../../components/StatsPanel";
import BottomTabs from "../../components/BottomTabs";

const PLAN_STAGES = [
  { id: "attach", index: "01", name: "建立会话", detail: "验证目标、模块与异常通道", status: "done", file: "bootstrap.py" },
  { id: "exception", index: "02", name: "异常分类", detail: "识别 VMP 单步异常并保持透传", status: "done", file: "exception_gate.py" },
  { id: "transition", index: "03", name: "执行转换监控", detail: "捕获 .text 写入与 RX 转换", status: "done", file: "memory_transition.py" },
  { id: "candidate", index: "04", name: "OEP 候选评分", detail: "交叉验证控制流与模块边界", status: "running", file: "score_oep.py" },
  { id: "capture", index: "05", name: "停止并采集", detail: "上下文、内存与证据快照", status: "waiting", file: "capture_evidence.py" },
  { id: "dump", index: "06", name: "转交转储", detail: "输出 OEP 及后续静态处理输入", status: "waiting", file: "handoff_dump.py" },
];

const ASSETS = [
  { id: "vmp-scripts", type: "script-group", name: "VMP OEP 脚本组", meta: "6 个脚本", status: "运行中" },
  { id: "bp-oep", type: "rule", kind: "断点", name: "OEP 候选断点", meta: "0x140008B70", status: "已启用" },
  { id: "ex-single-step", type: "rule", kind: "异常", name: "单步异常分流", meta: "0x80000004", status: "已启用" },
  { id: "hook-protect", type: "rule", kind: "函数 Hook", name: "NtProtectVirtualMemory", meta: "ntdll.dll", status: "已启用" },
  { id: "trace-text", type: "rule", kind: "Trace", name: ".text 执行转换", meta: "RX transition", status: "待命" },
];

const CODE = {
  "bootstrap.py": `from pinbridge import pb

target = pb.target()
main = pb.module(target.image_name)

workflow.share("image_base", main.base)
workflow.share("text_range", main.section(".text").range)
workflow.emit("session_ready", pid=target.pid, module=main.name)`,
  "exception_gate.py": `from pinbridge import pb

SINGLE_STEP = 0x80000004

@pb.intercept("exception.handle", codes=[SINGLE_STEP])
def route_vmp_exception(event):
    # VMP uses this exception as part of its state machine.
    # None means: keep the target's VEH / SEH path intact.
    workflow.observe("vmp_single_step", ip=event.ip, tid=event.thread_id)
    return None`,
  "memory_transition.py": `from pinbridge import pb

text = workflow.get("text_range")

@pb.hook("ntdll!NtProtectVirtualMemory")
def on_protect(call):
    region = call.args.BaseAddress.deref()
    protection = call.args.NewProtection
    if region.overlaps(text) and protection.executable:
        workflow.emit("text_rx", region=region, protection=protection)
        workflow.next("candidate")`,
  "score_oep.py": `from pinbridge import pb

@pb.on("execution.transition", ranges=[workflow.get("text_range")])
def score_candidate(event):
    score = 0
    score += 35 if event.from_private_memory else 0
    score += 30 if event.ip.in_main_image else 0
    score += 20 if event.stack.normalized else 0
    score += 15 if event.import_calls_visible else 0

    workflow.record_candidate(event.ip, score=score, evidence=event.evidence)
    if score >= 85:
        pb.breakpoint(event.ip, on_candidate, once=True)

def on_candidate(hit):
    workflow.checkpoint("candidate_oep", context=hit.context)
    return pb.action.stay`,
  "capture_evidence.py": `from pinbridge import pb

candidate = workflow.best_candidate()
snapshot = pb.capture(
    address=candidate.address,
    context=True,
    stack=True,
    memory=[candidate.module.image_range],
)
workflow.artifact("oep_evidence", snapshot)
workflow.next("dump")`,
  "handoff_dump.py": `candidate = workflow.best_candidate()

workflow.result(
    oep_va=candidate.address,
    oep_rva=candidate.address - workflow.get("image_base"),
    confidence=candidate.score,
    evidence=workflow.artifact("oep_evidence"),
)
# Dump / IAT repair is a separate downstream workflow.`,
};

const RULE_CODE = {
  "bp-oep": `from pinbridge import pb

@pb.breakpoint(0x140008B70, once=False)
def on_oep_candidate(hit):
    ctx = hit.context
    pb.log.info("OEP candidate reached", rip=ctx.rip, rsp=ctx.rsp)
    pb.capture(context=True, stack=True, memory=["main:.text"])
    return pb.action.stay`,
  "ex-single-step": `from pinbridge import pb

@pb.intercept(
    "exception.handle",
    codes=[0x80000004],
    thread_id=None,
    once=False,
)
def on_single_step(event):
    pb.log.debug("VMP single-step", ip=event.ip, tid=event.thread_id)
    # Passthrough: do not break the target's VEH / SEH state machine.
    return None`,
  "hook-protect": `from pinbridge import pb

@pb.hook("ntdll!NtProtectVirtualMemory")
def on_protect(call):
    base = call.args.BaseAddress.deref()
    size = call.args.RegionSize.deref()
    new_protect = call.args.NewProtection
    pb.log.info("memory protection", base=base, size=size, protect=new_protect)
    return pb.action.continue_`,
  "trace-text": `from pinbridge import pb

@pb.trace(
    modules=["crypto.vmp.exe"],
    sections=[".text"],
    events=["branch", "memory", "exception"],
    batch_size=4096,
)
def receive_trace(batch):
    workflow.consume("text_trace", batch)
`,
};

const ACTIVITIES = [
  { id: "op-0194", time: "14:32:18.406", actor: "AI", tool: "script.inject", purpose: "加载 OEP 候选评分阶段", args: "score_oep.py · generation 7", duration: "42 ms", result: "成功", parent: "op-0190", tone: "ok" },
  { id: "op-0193", time: "14:32:18.331", actor: "AI", tool: "breakpoint.set", purpose: "在最高分候选处建立一次性执行断点", args: "0x140008B70 · once=true", duration: "11 ms", result: "BP #12", parent: "op-0190", tone: "ok" },
  { id: "op-0192", time: "14:32:18.204", actor: "目标", tool: "execution.trap", purpose: "候选 OEP 命中，暂停目标并保留上下文", args: "TID 6480 · crypto.vmp.exe+.text", duration: "—", result: "已停止", parent: "op-0190", tone: "stop" },
  { id: "op-0191", time: "14:32:17.982", actor: "AI", tool: "memory.read", purpose: "读取候选入口附近原始证据", args: "0x140008B40 · 0x100 bytes", duration: "17 ms", result: "256 bytes", parent: "op-0190", tone: "ok" },
  { id: "op-0190", time: "14:32:17.610", actor: "AI", tool: "workflow.run_stage", purpose: "执行 OEP 候选评分阶段", args: "vmp-oep / stage 04", duration: "796 ms", result: "等待人工确认", parent: "—", tone: "wait" },
];

const EVIDENCE = [
  ["140008B58", "48 83 EC 28", "sub rsp, 28h", "建立正常栈帧"],
  ["140008B5C", "E8 2F 13 00 00", "call 0000000140009E90", "进入主模块函数"],
  ["140008B61", "48 8B D8", "mov rbx, rax", ""],
  ["140008B64", "48 85 C0", "test rax, rax", ""],
  ["140008B67", "74 0D", "je 0000000140008B76", ""],
  ["140008B69", "FF 15 91 84 00 00", "call qword ptr [KERNEL32!GetProcAddress]", "可见导入调用"],
  ["140008B6F", "90", "nop", ""],
  ["140008B70", "48 8B 4B 20", "mov rcx, qword ptr [rbx+20h]", "当前停止位置"],
];

export default function UnifiedWorkbench() {
  const [board, setBoard] = useState("home");
  const [controlMode, setControlMode] = useState("assist");
  const [selectedId, setSelectedId] = useState("vmp-scripts");
  const [selectedStage, setSelectedStage] = useState("candidate");
  const [activeFile, setActiveFile] = useState("score_oep.py");
  const [codeByFile, setCodeByFile] = useState(CODE);
  const [ruleCode, setRuleCode] = useState(RULE_CODE);
  const [evidenceOpen, setEvidenceOpen] = useState(false);
  const [batchOpen, setBatchOpen] = useState(false);
  const [activityId, setActivityId] = useState("op-0194");
  const [timelineFilter, setTimelineFilter] = useState("全部");

  const selected = ASSETS.find((item) => item.id === selectedId) || ASSETS[0];
  const stage = PLAN_STAGES.find((item) => item.id === selectedStage) || PLAN_STAGES[3];
  const activity = ACTIVITIES.find((item) => item.id === activityId) || ACTIVITIES[0];
  const isScriptGroup = selected.type === "script-group";

  const selectAsset = (id) => {
    const next = ASSETS.find((item) => item.id === id);
    setSelectedId(id);
    setEvidenceOpen(false);
    if (next?.type === "script-group") setActiveFile(stage.file);
  };

  const selectStage = (next) => {
    setSelectedStage(next.id);
    setActiveFile(next.file);
  };

  const editorValue = isScriptGroup ? codeByFile[activeFile] || "" : ruleCode[selected.id] || "";
  const updateEditor = (value) => {
    if (isScriptGroup) setCodeByFile((current) => ({ ...current, [activeFile]: value }));
    else setRuleCode((current) => ({ ...current, [selected.id]: value }));
  };

  const lineCount = useMemo(() => Math.max(16, editorValue.split("\n").length), [editorValue]);
  const filteredActivities = timelineFilter === "全部" ? ACTIVITIES : ACTIVITIES.filter((item) => item.actor === timelineFilter);

  const openBoard = (next) => {
    const targets = {
      target: "bp-oep",
      breakpoint: "bp-oep",
      exception: "ex-single-step",
      hook: "hook-protect",
      trace: "trace-text",
      scripts: "vmp-scripts",
      activity: "vmp-scripts",
    };
    if (targets[next]) setSelectedId(targets[next]);
    setBoard(next);
  };

  return (
    <div className="pbw-shell">
      <header className="pbw-topbar">
        {board !== "home" && <button className="pbw-top-back" onClick={() => setBoard("home")} aria-label="返回主页">←</button>}
        <button className="pbw-brand" onClick={() => setBoard("home")}><span className="pbw-brandmark">PB</span><span>PinBridge</span><small>ANALYSIS WORKSPACE</small></button>
        {board !== "home" && <div className="pbw-top-section"><span>/</span><b>{boardLabel(board)}</b></div>}
        <div className="pbw-target">
          <span className="pbw-live-dot" />
          <div><b>crypto.vmp.exe</b><span>PID 18436 · x64 · 会话 PB-0817-04</span></div>
        </div>
        <div className="pbw-stop-summary"><span>停止原因</span><b>执行断点 · <code>0x140008B70</code></b></div>
        <div className="pbw-control">
          <span className="pbw-control-label">控制权</span>
          <div className="pbw-segmented">
            <button className={controlMode === "manual" ? "active" : ""} onClick={() => setControlMode("manual")}>人工</button>
            <button className={controlMode === "assist" ? "active" : ""} onClick={() => setControlMode("assist")}>AI 辅助</button>
            <button className={controlMode === "auto" ? "active ai" : ""} onClick={() => setControlMode("auto")}>AI 自主</button>
          </div>
          {controlMode === "auto" && <button className="pbw-takeover" onClick={() => setControlMode("manual")}>立即接管</button>}
        </div>
        <div className="pbw-run-controls"><button title="暂停">Ⅱ&nbsp; 暂停</button><button className="primary" title="继续运行">▶&nbsp; 继续</button></div>
      </header>

      <div className="pbw-split-workspace">
        <section className="pbw-debugger-half">
          <div className="pbw-half-title"><div><b>传统调试器</b><span>汇编与原生断点</span></div><span><i />已停止 · RIP 0x140008B70</span></div>
          <TraditionalDebugger />
        </section>
        <section className="pbw-automation-half">
          {board === "home" ? <HomeDashboard onOpen={openBoard} /> : <>
            <div className="pbw-workspace-head">
              <div>
                <div className="pbw-breadcrumb">{boardLabel(board)} / {isScriptGroup ? "脚本组与运行状态" : `代码驱动规则 / ${selected.kind}`}</div>
                <h1>{board === "scripts" ? "动态脚本" : board === "activity" ? "AI / MCP 操作" : selected.name}</h1>
              </div>
              <div className="pbw-head-actions">
                <button onClick={() => setEvidenceOpen((value) => !value)}>{evidenceOpen ? "关闭证据" : "原始证据"}</button>
                <button className="primary">{isScriptGroup ? "继续脚本" : "应用规则"}</button>
              </div>
            </div>
            {board === "scripts" ? <ScriptBoard /> : board === "activity" ? <McpBoard activity={activity} /> : <RuleWorkspace rule={selected} />}
            {evidenceOpen && <EvidencePanel onClose={() => setEvidenceOpen(false)} />}
            <CodeEditor
              scriptGroup={isScriptGroup}
              stages={PLAN_STAGES}
              activeFile={activeFile}
              onFile={(file) => setActiveFile(file)}
              value={editorValue}
              onChange={updateEditor}
              lineCount={lineCount}
              rule={selected}
            />
          </>}
        </section>
      </div>
      <footer className="pbw-statusbar">
        <span><i className="connected" /> Agent 已连接</span><span>目标已停止</span><span>TID 6480</span><span>ABI 1.2</span><span>事件 1,284,392</span><span className="push">Python 3.13 · Agent x64 · 延迟 3 ms</span>
      </footer>

      {batchOpen && <BatchHookDialog onClose={() => setBatchOpen(false)} />}
    </div>
  );
}

function HomeDashboard({ onOpen }) {
  const tiles = [
    { id: "breakpoint", className: "breakpoint wide", kicker: "断点策略", title: "3 条策略", value: "2 条运行 · 1 条待命", chart: [0, 2, 1, 5, 2, 3, 8, 4, 9, 3], rows: [["最近命中", "0x140008B70"], ["线程", "TID 6480"], ["命中次数", "1"], ["回调", "on_oep_candidate"]], foot: "Python 回调 3 · 运行错误 0" },
    { id: "exception", className: "exception wide", kicker: "异常", title: "3 条规则", value: "0x80000004", chart: [2, 8, 4, 13, 7, 18, 11, 24, 15, 21], rows: [["处置", "透传 VEH / SEH"], ["最近位置", "0x7FFAE2113A42"], ["发生次数", "36"], ["处理器", "on_single_step"]], foot: "未吞掉目标异常" },
    { id: "hook", className: "hook wide", kicker: "Hook", title: "8 个", value: "7 个启用", chart: [1, 1, 4, 3, 7, 5, 12, 9, 18, 16], rows: [["最近函数", "NtProtectVirtualMemory"], ["命中次数", "18"], ["参数修改", "无"], ["返回策略", "执行原函数"]], foot: "高可信签名 6 / 8" },
    { id: "trace", className: "trace wide", kicker: "Trace", title: "1 个策略", value: ".text 执行转换", chart: [11, 29, 24, 41, 38, 58, 47, 65, 52, 72], rows: [["事件", "branch · memory"], ["已采集", "1,284,392"], ["当前速率", "42.8 K/s"], ["丢失", "0"]], foot: "批量 4,096 · 缓冲正常" },
    { id: "scripts", className: "scripts wide", kicker: "动态脚本", title: "VMP OEP 脚本组", value: "6 个脚本 · 当前 score_oep.py", chart: [18, 26, 21, 34, 28, 42, 31, 39], rows: [["运行", "5"], ["待命", "1"], ["回调", "14"], ["错误", "0"], ["最近热替换", "42 ms"]], foot: "候选 0x140008B70 · Gen 7 · 可回滚" },
    { id: "activity", className: "activity wide", kicker: "AI / MCP 活动", title: "候选 OEP 已捕获", value: "等待你的决定", chart: [9, 17, 12, 42, 11, 17, 26, 8], rows: [["最近工具", "script.inject"], ["参数", "score_oep.py · Gen 7"], ["结果", "成功 · 42 ms"], ["父操作", "op-0190"], ["调用链", "inject → bp.set → trap"]], foot: "所有调用参数、结果和父子关系均已记录" },
  ];
  return (
    <main className="pbw-home">
      <header className="pbw-home-head">
        <div><span>自动化工作面</span><h1>调试策略</h1><p>左侧保留原生调试器；这里管理代码驱动的策略、脚本和 AI 操作。</p></div>
        <div className="pbw-home-state"><i />目标已停止<b>候选 OEP 等待确认</b></div>
      </header>
      <div className="pbw-tile-grid">
        {tiles.map((tile, index) => (
          <button key={`${tile.id}-${index}`} className={`pbw-tile ${tile.className}`} onClick={() => onOpen(tile.id)}>
            <span className="pbw-tile-kicker">{tile.kicker}<b>↗</b></span>
            <strong>{tile.title}</strong>
            <em>{tile.value}</em>
            <TilePreview tile={tile} />
            {tile.progress != null && <div className="pbw-tile-progress"><i style={{ width: `${tile.progress}%` }} /></div>}
            <small>{tile.foot}</small>
          </button>
        ))}
      </div>
      <div className="pbw-home-foot"><span>界面原型 · 当前为模拟数据</span><span>人工与 AI 使用同一套断点、异常、Hook、Trace 和动态脚本资产</span></div>
    </main>
  );
}

function TilePreview({ tile }) {
  if (tile.id === "target") return (
    <div className="pbw-runtime-preview target">
      <div className="pbw-stop-line"><i className="pulse" /><code>RIP  0x140008B70</code><b>execution.trap</b></div>
      <div className="pbw-context-strip"><span><small>THREAD</small>TID 6480</span><span><small>MODULE</small>crypto.vmp.exe+.text</span><span><small>STACK</small>normalized</span></div>
      <div className="pbw-register-strip"><code>RAX 000001C8…</code><code>RSP 000000C4…</code><code>RFLAGS 0202</code></div>
    </div>
  );
  if (tile.className.includes("control")) return (
    <div className="pbw-runtime-preview control">
      <div className="pbw-control-flow"><span className="done">AI 建议</span><i>→</i><span className="done">生成脚本</span><i>→</i><span className="waiting">人工确认</span></div>
      <div className="pbw-live-caption"><i className="waiting" /><code>breakpoint.set</code><b>op-0194</b></div>
    </div>
  );
  if (tile.id === "breakpoint") return (
    <div className="pbw-runtime-preview event-list breakpoint">
      <div className="hit"><i className="pulse" /><code>OEP 候选</code><span>on_oep_candidate</span><b>stay</b></div>
      <div><i /><code>模块加载</code><span>on_module_ready</span><b>resume</b></div>
      <div><i /><code>权限转换</code><span>on_text_rx</span><b>observe</b></div>
    </div>
  );
  if (tile.id === "exception") return (
    <div className="pbw-runtime-preview event-list exception">
      <div className="live"><code>0x80000004</code><span>0x7FFAE2113A42</span><b>透传</b></div>
      <div><code>0x80000004</code><span>0x7FFAE21139F8</span><b>透传</b></div>
      <div><code>0xC0000005</code><span>0x140031820</span><b>观察</b></div>
    </div>
  );
  if (tile.id === "hook") return (
    <div className="pbw-runtime-preview hook-call">
      <div><i className="pulse" /><code>NtProtectVirtualMemory</code><b>18</b></div>
      <p><span>BaseAddress</span><code>0x140001000</code><span>Size</span><code>0x4D000</code></p>
      <p><span>Protect</span><code>RW → RX</code><span>Return</span><code>STATUS_SUCCESS</code></p>
      <small>参数未修改 · 原函数已执行</small>
    </div>
  );
  if (tile.id === "trace") return (
    <div className="pbw-runtime-preview trace-live">
      <MiniTrend values={tile.chart} />
      <div><span><i className="branch" />branch</span><span><i className="memory" />memory</span><b>42.8 K events/s</b></div>
      <small><code>crypto.vmp.exe:.text</code> · batch 4,096 · dropped 0</small>
    </div>
  );
  if (tile.id === "scripts") return (
    <div className="pbw-runtime-preview script-stack">
      <div><i className="running" /><code>score_oep.py</code><span>Gen 7 · callbacks 3</span><b>运行</b></div>
      <div><i className="running" /><code>exception_gate.py</code><span>Gen 4 · callbacks 1</span><b>运行</b></div>
      <div><i /><code>capture_evidence.py</code><span>Gen 2 · callbacks 2</span><b>待命</b></div>
    </div>
  );
  return (
    <div className="pbw-runtime-preview mcp-chain">
      <div><span className="done">script.inject<small>42 ms</small></span><i>→</i><span className="done">breakpoint.set<small>11 ms</small></span><i>→</i><span className="stop">execution.trap<small>stopped</small></span></div>
      <p><i className="waiting" />AI 已完成操作，目标保持停止，等待人工决定下一阶段</p>
    </div>
  );
}

function MiniTrend({ values }) {
  const width = 92;
  const height = 31;
  const max = Math.max(...values, 1);
  const min = Math.min(...values, 0);
  const range = Math.max(max - min, 1);
  const points = values.map((value, index) => `${(index / (values.length - 1)) * width},${height - 3 - ((value - min) / range) * (height - 7)}`).join(" ");
  const area = `0,${height} ${points} ${width},${height}`;
  return (
    <svg className="pbw-mini-trend" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" aria-hidden="true">
      <line x1="0" y1="10" x2={width} y2="10" /><line x1="0" y1="21" x2={width} y2="21" />
      <polygon points={area} />
      <polyline points={points} />
    </svg>
  );
}

function AssetExplorer({ board, selectedId, onSelect, onBatch }) {
  const rules = ASSETS.filter((item) => item.type === "rule");
  const boardRule = rules.filter((item) => ({ breakpoint: "断点", exception: "异常", hook: "函数 Hook", trace: "Trace" }[board] === item.kind));
  return (
    <aside className="pbw-explorer">
      <div className="pbw-pane-title"><span>{boardLabel(board)}</span><button title="新建资产">＋</button></div>
      <div className="pbw-project-card">
        <div className="pbw-project-top"><span className="pbw-project-icon">V</span><div><b>crypto.vmp.exe</b><span>动态分析任务</span></div></div>
        <div className="pbw-progress"><i style={{ width: "68%" }} /></div>
        <div className="pbw-project-meta"><span>候选 OEP 已捕获</span><b>68%</b></div>
      </div>

      {board === "activity" && <ExplorerGroup title="操作记录" count="5">
        <div className="pbw-tree-file"><span>OP</span>op-0194 · script.inject<i className="ok">成功</i></div>
        <div className="pbw-tree-file"><span>OP</span>op-0193 · breakpoint.set<i className="ok">成功</i></div>
        <div className="pbw-tree-file"><span>EV</span>op-0192 · execution.trap<i className="changed">停止</i></div>
      </ExplorerGroup>}
      {board === "target" && <>
        <ExplorerGroup title="线程" count="4">
          <div className="pbw-tree-file"><span>TH</span>TID 6480 · stopped<i className="changed">当前</i></div>
          <div className="pbw-tree-file"><span>TH</span>TID 7124 · suspended<i>等待</i></div>
          <div className="pbw-tree-file"><span>TH</span>TID 7368 · suspended<i>等待</i></div>
        </ExplorerGroup>
        <ExplorerGroup title="模块" count="23">
          <div className="pbw-tree-file"><span>PE</span>crypto.vmp.exe<i className="ok">0x140000000</i></div>
          <div className="pbw-tree-file"><span>PE</span>ntdll.dll<i>0x7FFAE20F0000</i></div>
          <div className="pbw-tree-file"><span>PE</span>kernel32.dll<i>0x7FFAE0C20000</i></div>
        </ExplorerGroup>
        <ExplorerGroup title="视图" count="4">
          <div className="pbw-tree-file"><span>V</span>反汇编<i>当前</i></div>
          <div className="pbw-tree-file"><span>V</span>内存映射<i /></div>
          <div className="pbw-tree-file"><span>V</span>调用栈<i /></div>
        </ExplorerGroup>
      </>}
      {["breakpoint", "exception", "hook", "trace"].includes(board) && <ExplorerGroup title={`${boardLabel(board)}规则`} count={String(boardRule.length)}>
        {boardRule.map((item) => <AssetItem key={item.id} item={item} active={selectedId === item.id} onClick={() => onSelect(item.id)} icon={item.kind === "断点" ? "●" : item.kind === "异常" ? "⚡" : item.kind === "Trace" ? "⌁" : "↪"} />)}
      </ExplorerGroup>}
      {board === "scripts" && <ExplorerGroup title="VMP OEP 脚本组" count="6">
        <div className="pbw-tree-file"><span>PY</span>bootstrap.py<i className="ok">已加载</i></div>
        <div className="pbw-tree-file"><span>PY</span>score_oep.py<i className="changed">Gen 7</i></div>
        <div className="pbw-tree-file"><span>PY</span>capture_evidence.py<i>待命</i></div>
      </ExplorerGroup>}
      {(board === "scripts" || board === "trace") && <ExplorerGroup title="采集与证据" count="3">
        <div className="pbw-tree-file"><span>EV</span>candidate_oep.ctx<i>14:32</i></div>
        <div className="pbw-tree-file"><span>EV</span>text_transition.json<i>24 KB</i></div>
      </ExplorerGroup>}

      <div className="pbw-quick-actions">
        <span>人工快捷操作</span>
        {board === "hook" && <button onClick={onBatch}>批量 Hook 导入 / 导出</button>}
        {board === "exception" && <button>新建异常拦截规则</button>}
        {board === "breakpoint" && <button>从当前地址建立断点</button>}
        {board === "trace" && <button>新建 Trace 策略</button>}
        {board === "scripts" && <button>新建动态脚本</button>}
        {board === "target" && <><button>转到表达式 / 地址</button><button>在当前地址设置断点</button></>}
      </div>
    </aside>
  );
}

function ExplorerGroup({ title, count, children }) {
  return <section className="pbw-explorer-group"><div className="pbw-group-head"><span>⌄ {title}</span><b>{count}</b></div>{children}</section>;
}

function AssetItem({ item, active, onClick, icon }) {
  return (
    <button className={`pbw-asset-item ${active ? "active" : ""}`} onClick={onClick}>
      <span className={`pbw-asset-icon ${item.kind === "异常" ? "warn" : ""}`}>{icon}</span>
      <span className="pbw-asset-copy"><b>{item.name}</b><small>{item.kind || "动态脚本组"} · {item.meta}</small></span>
      <i className={item.status === "已启用" || item.status === "运行中" ? "enabled" : ""} />
    </button>
  );
}

function TraditionalDebugger() {
  const rows = EVIDENCE.map((row, index) => ({
    address: `0x${row[0]}`,
    bytes: row[1].replaceAll(" ", ""),
    text: row[2],
    target: "0x0",
    kind: index === 1 ? 2 : 0,
  }));
  const regs = [
    [10, "0x000001C82A9F0000"], [7, "0x000001C82A8D41E0"], [9, "0x0000000000000001"],
    [8, "0x000001C82A9F1000"], [4, "0x0000000000000000"], [3, "0x000001C82A8D41E0"],
    [5, "0x000000C4F7EFF8A0"], [6, "0x000000C4F7EFF830"], [11, "0x0000000000000000"],
    [12, "0x0000000000000000"], [13, "0x0000000000000000"], [14, "0x00007FFAE2113A42"],
    [15, "0x0000000000000000"], [16, "0x0000000000000000"], [17, "0x0000000000000000"],
    [18, "0x0000000000000000"], [26, "0x0000000140008B70"], [25, "0x0000000000000202"],
  ].map(([reg, value]) => ({ reg, value }));
  return (
    <section className="pbw-legacy-target">
      <Toolbar
        tid={6480}
        status={{ text: "Stopped @ 0x140008B70", err: false }}
        target="crypto.vmp.exe"
        onKillSession={() => {}}
        onStop={() => {}}
        onFollowRip={() => {}}
        onGoto={() => {}}
      />
      <div id="main">
        <DisasmView rows={rows} rip="0x140008b70" bpSet={new Set(["0x140008b5c", "0x140008b70"])} onSetBp={() => {}} onPage={() => {}} onPageUp={() => {}} />
        <div id="right">
          <Registers tid={6480} regs={regs} onChanged={() => {}} />
          <StatsPanel />
        </div>
      </div>
      <BottomTabs tid={6480} stopTick={1} onGoto={() => {}} />
    </section>
  );
}

function RuleWorkspace({ rule }) {
  const exception = rule.id === "ex-single-step";
  const breakpoint = rule.id === "bp-oep";
  const hook = rule.id === "hook-protect";
  return (
    <section className="pbw-rule-area">
      <div className="pbw-rule-banner">
        <div><span className="pbw-card-label">{breakpoint ? "代码驱动断点策略" : "单点规则"}</span><h2>{breakpoint ? "触发条件 + Python 回调 + 停止动作" : `${rule.kind}触发器 + Python 处理函数`}</h2><p>{breakpoint ? "与左侧人工原生断点分开管理；策略负责条件过滤、回调和自动处置。" : "表单只维护注册参数，下面的自定义函数体独立保存，不会被低代码配置覆盖。"}</p></div>
        <label className="pbw-switch"><input type="checkbox" defaultChecked /><i /><span>规则已启用</span></label>
      </div>
      <div className="pbw-form-grid">
        <Field label={exception ? "异常编号" : breakpoint ? "断点地址" : hook ? "目标函数" : "采集范围"} value={rule.meta} mono />
        <Field label="执行方式" value={exception || hook ? "同步拦截" : breakpoint ? "停止并等待回调" : "原生采集 / 批量回调"} />
        <Field label="线程过滤" value="所有线程" />
        <Field label="命中策略" value={breakpoint ? "持续有效" : "条件满足时处理"} />
        {exception && <><Field label="默认处置" value="透传到目标 VEH / SEH" important /><Field label="上下文修改" value="仅回调明确返回 registers 时" /></>}
        {hook && <><Field label="签名来源" value="Windows 类型库 · 高可信" important /><Field label="原函数" value="默认继续执行" /></>}
        {!exception && !hook && <><Field label="原生过滤" value="模块 + 地址范围" /><Field label="回调所有者" value="人工 · AI 可建议" /></>}
      </div>
      <div className="pbw-form-note"><span>注册参数</span>可以由界面生成；<span>处理逻辑</span>保留为完整 Python 源码，可热替换、回滚和版本对比。</div>
    </section>
  );
}

function Field({ label, value, mono, important }) {
  return <label className={`pbw-field ${important ? "important" : ""}`}><span>{label}</span><div className={mono ? "mono" : ""}>{value}<b>⌄</b></div></label>;
}

function ScriptBoard() {
  return (
    <section className="pbw-script-board">
      <div><span>已加载</span><b>6</b><small>5 个运行 · 1 个待命</small></div>
      <div><span>当前版本</span><b>Gen 7</b><small>sha 5c2a71 · 可回滚</small></div>
      <div><span>注册回调</span><b>14</b><small>异常 3 · Hook 5 · Trace 2</small></div>
      <div><span>运行错误</span><b>0</b><small>最近热替换用时 42 ms</small></div>
    </section>
  );
}

function McpBoard({ activity }) {
  return (
    <section className="pbw-mcp-board">
      <article><span>当前操作</span><h2>{activity.purpose}</h2><code>{activity.tool}({activity.args})</code></article>
      <article><span>执行关系</span><h2>{activity.id}</h2><p>父操作 {activity.parent} · AI 发起 · 人工可立即接管</p></article>
      <article><span>结果</span><h2>{activity.result}</h2><p>{activity.duration} · 完整参数和结果保留在下方时间线</p></article>
    </section>
  );
}

function boardLabel(board) {
  return {
    target: "传统调试器",
    breakpoint: "断点策略",
    exception: "异常",
    hook: "Hook",
    trace: "Trace",
    scripts: "动态脚本",
    activity: "AI / MCP",
  }[board] || "总览";
}

function CodeEditor({ scriptGroup, stages, activeFile, onFile, value, onChange, lineCount, rule }) {
  const files = scriptGroup ? stages.map((stage) => stage.file) : [`${rule.id}.py`];
  return (
    <section className="pbw-editor">
      <div className="pbw-editor-tabs">
        {files.slice(scriptGroup ? 1 : 0, scriptGroup ? 5 : 1).map((file) => <button key={file} className={(scriptGroup ? activeFile === file : true) ? "active" : ""} onClick={() => onFile(file)}><span>PY</span>{file}{file === "score_oep.py" && <i>●</i>}</button>)}
        <div className="pbw-editor-actions"><span>Python · Gen 7 · 已加载</span><button>生成差异</button><button className="primary">保存并热替换</button></div>
      </div>
      <div className="pbw-editor-subbar">
        <span>{scriptGroup ? `脚本组 / ${activeFile}` : `规则处理函数 / ${rule.name}`}</span>
        <div><button>格式化</button><button>回滚</button><button>停止脚本</button></div>
      </div>
      <div className="pbw-code-wrap">
        <pre className="pbw-line-numbers">{Array.from({ length: lineCount }, (_, index) => index + 1).join("\n")}</pre>
        <textarea spellCheck="false" value={value} onChange={(event) => onChange(event.target.value)} aria-label="Python source editor" />
      </div>
    </section>
  );
}

function EvidencePanel({ onClose }) {
  return (
    <section className="pbw-evidence">
      <div className="pbw-evidence-head"><div><b>原始证据层</b><span>仅在需要验证判断时展开 · 当前地址附近反汇编</span></div><button onClick={onClose}>收起</button></div>
      <table><tbody>{EVIDENCE.map((row) => <tr key={row[0]} className={row[0] === "140008B70" ? "current" : ""}><td>{row[0]}</td><td>{row[1]}</td><td>{row[2]}</td><td>{row[3]}</td></tr>)}</tbody></table>
    </section>
  );
}

function Inspector({ board, selected, activity }) {
  const isHook = selected.id === "hook-protect";
  return (
    <aside className="pbw-inspector">
      <div className="pbw-pane-title"><span>详细检查器</span><button>···</button></div>
      <div className="pbw-inspector-tabs"><button className="active">详情</button><button>运行</button><button>历史</button></div>
      {board === "target" ? (
        <>
          <InspectorSection title="寄存器">
            <KeyValue label="RIP" value="0000000140008B70" mono tone="accent" />
            <KeyValue label="RSP" value="000000C4F7EFF830" mono />
            <KeyValue label="RBP" value="000000C4F7EFF8A0" mono />
            <KeyValue label="RAX" value="000001C82A9F0000" mono />
            <KeyValue label="RBX" value="000001C82A8D41E0" mono />
            <KeyValue label="RCX" value="0000000000000001" mono />
            <KeyValue label="RDX" value="000001C82A9F1000" mono />
            <KeyValue label="RFLAGS" value="0000000000000202" mono />
          </InspectorSection>
          <InspectorSection title="当前停止">
            <KeyValue label="原因" value="execution.trap" mono />
            <KeyValue label="线程" value="TID 6480" mono />
            <KeyValue label="模块" value="crypto.vmp.exe" mono />
            <KeyValue label="地址" value="0x140008B70" mono />
            <KeyValue label="断点" value="#12 · on_oep_candidate" />
          </InspectorSection>
        </>
      ) : board === "scripts" ? (
        <>
          <InspectorSection title="当前脚本">
            <KeyValue label="脚本组" value="VMP OEP" />
            <KeyValue label="脚本" value="score_oep.py" mono />
            <KeyValue label="执行主体" value="AI · 人工可接管" />
            <KeyValue label="状态" value="运行中 · 等待人工确认" tone="accent" />
            <KeyValue label="版本" value="Gen 7 · sha 5c2a71" mono />
          </InspectorSection>
          <InspectorSection title="脚本运行状态">
            <KeyValue label="image_base" value="0x140000000" mono />
            <KeyValue label="text_range" value="0x140001000–0x14004E000" mono />
            <KeyValue label="best_candidate" value="0x140008B70 / 92" mono />
            <KeyValue label="exception_policy" value="0x80000004 · passthrough" mono />
          </InspectorSection>
          <InspectorSection title="输出与证据">
            <Artifact state="ready" name="text_transition" meta="事件流 · 36 条" />
            <Artifact state="ready" name="candidate_oep.ctx" meta="上下文快照 · 4 KB" />
            <Artifact state="waiting" name="oep_evidence" meta="等待当前脚本继续" />
          </InspectorSection>
          <InspectorSection title="热替换与恢复">
            <KeyValue label="发生错误" value="停止脚本，目标保持停止" />
            <KeyValue label="可回滚" value="Gen 6" />
            <KeyValue label="保存状态" value="candidate_oep" mono />
          </InspectorSection>
        </>
      ) : board === "activity" ? (
        <>
          <InspectorSection title="当前操作">
            <KeyValue label="工具" value={activity.tool} mono />
            <KeyValue label="状态" value={activity.result} tone="accent" />
            <KeyValue label="主体" value={activity.actor} />
            <KeyValue label="操作编号" value={activity.id} mono />
            <KeyValue label="父操作" value={activity.parent} mono />
          </InspectorSection>
          <InspectorSection title="执行参数">
            <KeyValue label="目的" value={activity.purpose} />
            <KeyValue label="参数" value={activity.args} mono />
            <KeyValue label="耗时" value={activity.duration} />
          </InspectorSection>
        </>
      ) : (
        <>
          <InspectorSection title="规则身份">
            <KeyValue label="类型" value={selected.kind} />
            <KeyValue label="所有者" value="人工" />
            <KeyValue label="状态" value={selected.status} tone="accent" />
            <KeyValue label="脚本代次" value="Gen 7 · sha 5c2a71" mono />
            <KeyValue label="命中" value="18 次 · 最近 14:32:18" />
          </InspectorSection>
          {isHook ? <HookDetail /> : <RuleDetail selected={selected} />}
          <InspectorSection title="来源与审计">
            <KeyValue label="创建者" value="人工" />
            <KeyValue label="最近修改" value="AI 建议 · 人工确认" />
            <KeyValue label="操作编号" value="op-0182" mono />
          </InspectorSection>
        </>
      )}
      <InspectorSection title="选中的 MCP 操作">
        <KeyValue label="工具" value={activity.tool} mono />
        <KeyValue label="目的" value={activity.purpose} />
        <KeyValue label="参数" value={activity.args} mono />
        <KeyValue label="操作编号" value={activity.id} mono />
        <KeyValue label="父操作" value={activity.parent} mono />
        <KeyValue label="耗时 / 结果" value={`${activity.duration} · ${activity.result}`} tone="accent" />
      </InspectorSection>
    </aside>
  );
}

function InspectorSection({ title, children }) {
  return <section className="pbw-inspector-section"><h3>{title}<button>⌃</button></h3>{children}</section>;
}

function KeyValue({ label, value, mono, tone }) {
  return <div className="pbw-kv"><span>{label}</span><b className={`${mono ? "mono" : ""} ${tone || ""}`}>{value}</b></div>;
}

function Artifact({ state, name, meta }) {
  return <div className="pbw-artifact"><i className={state} /> <div><b>{name}</b><span>{meta}</span></div><button>↗</button></div>;
}

function HookDetail() {
  return <InspectorSection title="函数签名与拦截"><div className="pbw-trust"><span>签名来源</span><b>Windows 类型库</b><i>高可信</i></div><code className="pbw-signature">NTSTATUS NtProtectVirtualMemory(<br />&nbsp; HANDLE ProcessHandle,<br />&nbsp; PVOID *BaseAddress,<br />&nbsp; PSIZE_T RegionSize,<br />&nbsp; ULONG NewProtection,<br />&nbsp; PULONG OldProtection<br />)</code><KeyValue label="拦截位置" value="ntdll!NtProtectVirtualMemory" mono /><KeyValue label="入口指令" value="4C 8B D1 · mov r10, rcx" mono /><KeyValue label="允许修改" value="参数 / 返回值 / 跳过原函数" /><KeyValue label="当前动作" value="记录参数，原函数继续" tone="accent" /></InspectorSection>;
}

function RuleDetail({ selected }) {
  const exception = selected.id === "ex-single-step";
  return <InspectorSection title={exception ? "异常处置" : "触发与动作"}><KeyValue label="目标" value={selected.meta} mono /><KeyValue label="原生过滤" value={exception ? "code == 0x80000004" : "main module · all threads"} mono /><KeyValue label="回调模式" value={exception ? "同步拦截" : "停止后调用 Python"} /><KeyValue label="默认动作" value={exception ? "透传目标 VEH / SEH" : "stay · 等待决定"} tone="accent" /><KeyValue label="上下文写回" value="仅显式返回时" /></InspectorSection>;
}

function ActivityTimeline({ items, selected, onSelect, filter, onFilter }) {
  return (
    <section className="pbw-timeline">
      <div className="pbw-timeline-head">
        <div><b>结构化活动时间线</b><span>AI 与人工的每一次调试动作、MCP 调用和目标事件</span></div>
        <div className="pbw-filter">{["全部", "AI", "人工", "目标"].map((name) => <button key={name} className={filter === name ? "active" : ""} onClick={() => onFilter(name)}>{name}</button>)}<button>导出审计记录</button></div>
      </div>
      <div className="pbw-timeline-table">
        <div className="pbw-activity-row header"><span>时间 / 主体</span><span>MCP / 事件</span><span>目的</span><span>关键参数</span><span>耗时 / 结果</span><span>操作链</span></div>
        {items.map((item) => <button key={item.id} className={`pbw-activity-row ${selected === item.id ? "selected" : ""}`} onClick={() => onSelect(item.id)}><span><code>{item.time}</code><b className={`actor ${item.actor}`}>{item.actor}</b></span><span><code className="tool">{item.tool}</code></span><span>{item.purpose}</span><span><code>{item.args}</code></span><span><b className={`result ${item.tone}`}>{item.result}</b><small>{item.duration}</small></span><span><code>{item.id}</code><small>parent {item.parent}</small></span></button>)}
      </div>
    </section>
  );
}

function BatchHookDialog({ onClose }) {
  const [source, setSource] = useState("imports");
  return (
    <div className="pbw-dialog-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="pbw-dialog">
        <header><div><span>批量规则向导</span><h2>Hook 模块函数</h2></div><button onClick={onClose}>×</button></header>
        <div className="pbw-dialog-body">
          <label><span>目标模块</span><div className="pbw-select">crypto.vmp.exe <b>⌄</b></div></label>
          <label><span>函数来源</span><div className="pbw-source-tabs"><button className={source === "imports" ? "active" : ""} onClick={() => setSource("imports")}>导入函数</button><button className={source === "exports" ? "active" : ""} onClick={() => setSource("exports")}>EXE 导出</button><button className={source === "dll" ? "active" : ""} onClick={() => setSource("dll")}>DLL 全部导出</button></div></label>
          <div className="pbw-dialog-grid"><Field label="Hook 位置" value={source === "imports" ? "IAT 目标函数" : "函数入口"} /><Field label="调用方式" value="观察；原函数继续" /><Field label="签名策略" value="类型库优先，未知回退原始 ABI" /><Field label="过滤" value="排除转发与数据导出" /></div>
          <div className="pbw-batch-preview"><div><span>预览结果</span><b>{source === "imports" ? "47" : source === "exports" ? "3" : "1,842"} 个函数</b></div><div><span>高可信签名</span><b>{source === "imports" ? "39 / 47" : source === "exports" ? "1 / 3" : "1,208 / 1,842"}</b></div><div><span>预计规则</span><b>{source === "imports" ? "47" : source === "exports" ? "3" : "1,842"}</b></div><div className="risk"><span>性能风险</span><b>{source === "dll" ? "高 · 建议增加过滤" : "低"}</b></div></div>
          <div className="pbw-dialog-note">将创建一个“批量 Hook 规则集”，每个函数的签名、参数、命中和修改动作都可以单独查看；生成的 Python 注册代码保持可见。</div>
        </div>
        <footer><button onClick={onClose}>取消</button><button>查看函数清单</button><button className="primary">生成规则与代码</button></footer>
      </section>
    </div>
  );
}
