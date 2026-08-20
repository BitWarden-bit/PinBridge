import React, { useCallback, useEffect, useReducer, useRef, useState } from "react";
import Toolbar from "../../components/Toolbar";
import DisasmView from "../../components/DisasmView";
import Registers from "../../components/Registers";
import BottomTabs from "../../components/BottomTabs";
import LaunchWorkspace from "./LaunchWorkspace";
import AutomationPane from "./AutomationPane";
import { api } from "../../api";
import { normalizeAddress } from "../../address";
import { clearResolveCache } from "../../resolve";
import { getSnapshot, subscribe } from "../../store";

// One shared human/AI workspace. The left half is the real debugger wired to
// the live Agent session. The right automation half was cleared and is being
// redesigned from scratch — no prototype boards or simulated data remain.
export default function UnifiedWorkbench() {
  const [session, setSession] = useState({ running: false, target: null });
  const [sessionChecked, setSessionChecked] = useState(false);
  const [status, setStatus] = useState({ text: "", err: false });
  const [tid, setTid] = useState(null);
  const [regs, setRegs] = useState([]);
  const [rows, setRows] = useState([]);
  const [stopTick, setStopTick] = useState(0);
  const [activities, setActivities] = useState([]);
  const [breakpointInventory, setBreakpointInventory] = useState([]);
  const [focusedAddress, setFocusedAddress] = useState(null);
  const [controlBusy, setControlBusy] = useState(false);
  const [controlMode, setControlMode] = useState("manual");
  const [followAi, setFollowAi] = useState(true);
  const [, forceSnapshot] = useReducer((value) => value + 1, 0);
  const previousStopGeneration = useRef("0");
  const snapshot = getSnapshot();

  useEffect(() => subscribe(forceSnapshot), []);

  useEffect(() => {
    const receiveError = (event) => setStatus({ text: event.detail, err: true });
    window.addEventListener("pb-error", receiveError);
    return () => window.removeEventListener("pb-error", receiveError);
  }, []);

  useEffect(() => {
    let live = true;
    api.session().then((value) => {
      if (!live) return;
      if (value) setSession(value);
      setSessionChecked(true);
    });
    return () => { live = false; };
  }, []);

  const loadDisasm = useCallback(async (address) => {
    const panel = document.getElementById("disasm");
    const count = panel ? Math.max(8, Math.floor((panel.clientHeight - 34) / 19)) : 32;
    const data = await api.disasm(address, count);
    if (data?.length) setRows(data);
  }, []);

  const loadDisasmUp = useCallback(async (address) => {
    const panel = document.getElementById("disasm");
    const count = panel ? Math.max(8, Math.floor((panel.clientHeight - 34) / 19)) : 32;
    const data = await api.disasmUp(address, count);
    if (data?.length) setRows(data);
  }, []);

  const gotoDisasm = useCallback(async (address) => {
    const normalized = normalizeAddress(address);
    if (!normalized) return;
    setFocusedAddress(normalized);
    await loadDisasm(normalized);
  }, [loadDisasm]);

  const loadContext = useCallback(async (targetTid) => {
    if (targetTid == null) return;
    const data = await api.context(targetTid);
    if (data) setRegs(data);
  }, []);

  const refreshStopped = useCallback(async (tidHint) => {
    const ids = await api.threads();
    if (ids?.length) {
      const selectedTid = tidHint != null && tidHint !== 0xffffffff && ids.includes(tidHint) ? tidHint : ids[0];
      setTid(selectedTid);
      const context = await api.context(selectedTid);
      if (context) {
        setRegs(context);
        const instructionPointer = context.find((entry) => entry.reg === 26);
        const address = normalizeAddress(instructionPointer?.value);
        if (address) loadDisasm(address);
      }
    }
    setStopTick((value) => value + 1);
  }, [loadDisasm]);

  const followRip = useCallback(async () => {
    if (tid == null) return;
    const context = await api.context(tid);
    if (!context) return;
    setRegs(context);
    const address = normalizeAddress(context.find((entry) => entry.reg === 26)?.value);
    if (address) loadDisasm(address);
  }, [loadDisasm, tid]);

  const setBreakpoint = useCallback(async (address) => {
    await api.bpSet(address);
    setStatus({ text: `Breakpoint @ ${address}`, err: false });
  }, []);

  const killSession = useCallback(async () => {
    if (!window.confirm("终止目标进程和 Pin 后端？如果只是暂时离开，请使用“释放连接”。")) return;
    await api.killBackend();
    clearResolveCache();
    setSession({ running: false, target: null });
    setRegs([]);
    setRows([]);
    setTid(null);
    setActivities([]);
    setBreakpointInventory([]);
    setFocusedAddress(null);
  }, []);

  const releaseSession = useCallback(async () => {
    const result = await api.releaseSession();
    if (!result.ok) {
      setStatus({ text: result.error, err: true });
      return;
    }
    const released = result.value || {};
    if (released.address) localStorage.setItem("pb-agent-address", released.address);
    clearResolveCache();
    setSession({
      running: false,
      released: true,
      address: released.address,
      target: released.target,
      backend_owned: released.backend_owned,
    });
    setRegs([]);
    setRows([]);
    setTid(null);
    setActivities([]);
    setBreakpointInventory([]);
    setFocusedAddress(null);
  }, []);

  // Every completed stop bumps the agent's stop generation counter. Keying the
  // refresh off the counter — not a running->stopped edge — catches stop/run/
  // stop cycles that complete inside one poll window.
  const stopGeneration = snapshot.connected ? String(snapshot.stopGen ?? "0") : "0";
  useEffect(() => {
    if (stopGeneration === previousStopGeneration.current) return;
    previousStopGeneration.current = stopGeneration;
    if (stopGeneration !== "0" && snapshot.stopped) {
      const hitTid = Number(snapshot.hitTid);
      refreshStopped(Number.isSafeInteger(hitTid) ? hitTid : null);
    }
  }, [refreshStopped, snapshot.hitTid, snapshot.stopped, stopGeneration]);

  // Hub activity + control mode polling. A real in-flight AI operation drives
  // the shared live banner and the disassembly address highlight.
  const refreshHub = useCallback(async () => {
    const [activityResult, controlResult, breakpointResult] = await Promise.all([
      api.ai.activityList({ limit: "20" }),
      api.ai.controlStatus(),
      api.breakpointInventory(),
    ]);
    if (activityResult.ok) setActivities(Array.isArray(activityResult.value?.activities) ? activityResult.value.activities : []);
    if (controlResult.ok && controlResult.value?.mode) {
      const mode = controlResult.value.mode;
      setControlMode(["ai", "ai_read_only", "ai_assist", "ai_autonomous", "automation_paused"].includes(mode) ? "auto" : "manual");
    }
    if (breakpointResult.ok) {
      setBreakpointInventory(Array.isArray(breakpointResult.value?.breakpoints) ? breakpointResult.value.breakpoints : []);
    }
  }, []);

  useEffect(() => {
    if (!session.running || !snapshot.connected) return undefined;
    let live = true;
    const run = async () => {
      if (live) await refreshHub();
    };
    run();
    const timer = window.setInterval(run, 3500);
    return () => {
      live = false;
      window.clearInterval(timer);
    };
  }, [session.running, snapshot.connected, refreshHub]);

  const changeControlMode = useCallback(async (nextMode) => {
    if (controlBusy) return;
    const previousMode = controlMode;
    setControlMode(nextMode);
    setControlBusy(true);
    const result = nextMode === "auto" ? await api.ai.handoffToAi() : await api.ai.takeoverManual();
    setControlBusy(false);
    if (!result.ok) {
      setControlMode(previousMode);
      setStatus({ text: result.error, err: true });
    }
  }, [controlBusy, controlMode]);

  const aiActive = controlMode !== "manual";
  const aiOp = aiActive ? activityToAiOp(activities.find(activityInFlight)) : null;

  if (!sessionChecked) {
    return <div className="pbw-shell"><div className="pbw-launch-loading">正在读取本机会话…</div></div>;
  }

  if (!session.running) {
    return (
      <div className="pbw-shell">
        <LaunchWorkspace releasedSession={session.released ? session : null} onLaunch={(value) => setSession({ running: true, target: value.target, released: false })} />
      </div>
    );
  }

  const rip = normalizeAddress(regs.find((entry) => entry.reg === 26)?.value)
    || normalizeAddress(snapshot.hitAddr)
    || "0x0";
  const bpSet = new Set((snapshot.bps || []).map((breakpoint) => normalizeAddress(breakpoint.address)).filter(Boolean));
  const targetLabel = fileName(session.target) || "当前目标";
  const targetStopped = !!snapshot.stopped;
  const pidLabel = snapshot.pid && snapshot.pid !== "0" ? String(snapshot.pid) : "—";
  const abiLabel = `${snapshot.abi?.[0] ?? "0"}.${snapshot.abi?.[1] ?? "0"}`;

  return (
    <div className="pbw-shell">
      <header className="pbw-topbar">
        <div className="pbw-brand"><span className="pbw-brandmark">PB</span><span>PinBridge</span><small>ANALYSIS WORKSPACE</small></div>
        <div className="pbw-target">
          <span className="pbw-live-dot" />
          <div><b>{targetLabel}</b><span>PID {pidLabel} · x64 · {snapshot.connected ? "Agent 已连接" : "等待 Agent"}</span></div>
        </div>
        <div className="pbw-stop-summary"><span>目标状态</span><b>{targetStopped ? "已停止" : "运行中"}{targetStopped && rip !== "0x0" ? <> · <code>{rip}</code></> : null}</b></div>
        <div className="pbw-control">
          <span className="pbw-control-label">控制权</span>
          <div className="pbw-segmented">
            <button disabled={controlBusy} className={controlMode === "manual" ? "active" : ""} onClick={() => changeControlMode("manual")}>人工</button>
            <button disabled={controlBusy} className={controlMode === "auto" ? "active ai" : ""} onClick={() => changeControlMode("auto")}>AI 全自动</button>
          </div>
          {controlMode === "auto" && <button className="pbw-takeover" disabled={controlBusy} onClick={() => changeControlMode("manual")}>立即接管</button>}
        </div>
        <button
          className={`pbw-follow ${followAi && aiActive ? "on" : ""}`}
          disabled={!aiActive}
          title={aiActive ? "自动定位 AI 操作地址" : "AI 未运行"}
          onClick={() => setFollowAi((value) => !value)}
        >
          <i />跟随 AI
        </button>
        <div className="pbw-run-controls">
          <button title="释放工作区控制，保留目标与 Pin/Agent" onClick={releaseSession}>⏏&nbsp; 释放</button>
          <button title="暂停" onClick={async () => { await api.control("stop"); refreshStopped(); }}>Ⅱ&nbsp; 暂停</button>
          <button className="primary" title="继续运行" onClick={() => api.control("resume")}>▶&nbsp; 继续</button>
        </div>
      </header>

      <div className="pbw-split-workspace">
        <section className="pbw-debugger-half">
          <div className="pbw-half-title"><div><b>传统调试器</b><span>汇编 · 寄存器 · 内存</span></div><span><i className={targetStopped ? "stopped" : ""} />{targetStopped ? "已停止" : "运行中"} · RIP {rip}</span></div>
          {aiOp && (
            <div className={`pbw-ai-live ${followAi ? "" : "paused"}`}>
              <i />
              <code>{aiOp.tool}</code>
              <span>{aiOp.args}</span>
              <b>{aiOp.id}</b>
              <em>{followAi ? "左侧跟随中" : "跟随已暂停"}</em>
            </div>
          )}
          <TraditionalDebugger
            aiOp={aiOp}
            followAi={followAi}
            focusedAddress={focusedAddress}
            rows={rows}
            regs={regs}
            tid={tid}
            rip={rip}
            bpSet={bpSet}
            status={status}
            target={session.target}
            snapshot={snapshot}
            stopTick={stopTick}
            onKillSession={killSession}
            onReleaseSession={releaseSession}
            onStop={refreshStopped}
            onFollowRip={followRip}
            onGoto={gotoDisasm}
            onPage={loadDisasm}
            onPageUp={loadDisasmUp}
            onSetBp={setBreakpoint}
            onRegistersChanged={() => loadContext(tid)}
          />
        </section>
        <section className="pbw-automation-half">
          <div className="pbw-half-title"><div><b>分析控制</b><span>断点 · 异常 · Hook</span></div></div>
          <AutomationPane
            rip={rip}
            stopped={targetStopped}
            hitAddr={snapshot.hitAddr}
            bps={breakpointInventory.length > 0 ? breakpointInventory : (snapshot.bps || [])}
            onGoto={gotoDisasm}
            onRefreshBreakpoints={refreshHub}
            activities={activities}
            onRefreshActivities={refreshHub}
            stopTick={stopTick}
          />
        </section>
      </div>

      <footer className="pbw-statusbar">
        <span><i className={snapshot.connected ? "connected" : ""} /> {snapshot.connected ? "Agent 已连接" : "Agent 未连接"}</span>
        <span>{targetStopped ? "目标已停止" : "目标运行中"}</span>
        <span>TID {tid ?? "—"}</span>
        <span>ABI {abiLabel}</span>
        <span>事件 {formatCounter(snapshot.total)}</span>
        {aiOp && <span className="pbw-status-ai"><i className="ai" />AI {aiOp.tool} · {aiOp.args}</span>}
        <span className="push">{targetLabel} · Agent x64 · 丢失 {formatCounter(snapshot.dropped)}</span>
      </footer>
    </div>
  );
}

function TraditionalDebugger({
  aiOp, followAi, focusedAddress, rows, regs, tid, rip, bpSet, status, target,
  snapshot, stopTick, onKillSession, onReleaseSession, onStop, onFollowRip, onGoto, onPage,
  onPageUp, onSetBp, onRegistersChanged,
}) {
  // Breakpoint ownership: human-placed native breakpoints and breakpoints the
  // AI just placed are distinct assets and must read differently in the gutter.
  const bpOwners = Object.fromEntries(Array.from(bpSet, (address) => [address, "human"]));
  const aiAddr = normalizeAddress(aiOp?.addr);
  if (aiOp?.setsBp && aiAddr) bpOwners[aiAddr] = "ai";
  return (
    <section className="pbw-legacy-target">
      <Toolbar
        tid={tid}
        status={status}
        target={target}
        onKillSession={onKillSession}
        onReleaseSession={onReleaseSession}
        onStop={onStop}
        onFollowRip={onFollowRip}
        onGoto={onGoto}
      />
      <div id="main">
        <DisasmView rows={rows} rip={rip} bpSet={bpSet} bpOwners={bpOwners} aiAddr={aiAddr} followAddr={followAi ? aiAddr : null} focusAddr={focusedAddress} onSetBp={onSetBp} onPage={onPage} onPageUp={onPageUp} />
        <div id="right">
          <Registers tid={tid} regs={regs} onChanged={onRegistersChanged} />
          <SessionScene aiOp={aiOp} snapshot={snapshot} target={target} tid={tid} rip={rip} />
        </div>
      </div>
      <BottomTabs tid={tid} stopTick={stopTick} onGoto={onGoto} tabs={["mem", "stack", "mods"]} />
    </section>
  );
}

// Compact session-scene panel next to the registers: why the target stopped
// and on which thread/module. Breakpoint ownership lives in the disassembly
// gutter, not here.
function SessionScene({ aiOp, snapshot, target, tid, rip }) {
  const stopped = !!snapshot.stopped;
  return (
    <div className="pbw-scene">
      <div className="pbw-scene-title">会话现场</div>
      <div className="pbw-scene-kv"><span>状态</span><b className="mono">{stopped ? "已停止" : "运行中"}</b></div>
      <div className="pbw-scene-kv"><span>位置</span><b className="mono">{rip}</b></div>
      <div className="pbw-scene-kv"><span>线程</span><b className="mono">TID {tid ?? "—"}</b></div>
      <div className="pbw-scene-kv"><span>模块</span><b className="mono">{fileName(target) || "—"}</b></div>
      {aiOp && <>
        <div className="pbw-scene-title">AI 操作</div>
        <div className="pbw-scene-kv"><span>工具</span><b className="mono">{aiOp.tool}</b></div>
        <div className="pbw-scene-kv"><span>编号</span><b className="mono">{aiOp.id}</b></div>
      </>}
    </div>
  );
}

function activityInFlight(activity) {
  return !!activity && !activity.completed_at_ms && activity.outcome === "in_progress";
}

function activityToAiOp(activity) {
  if (!activity) return null;
  const address = normalizeAddress(activity.resource_refs?.address) || addressFromValue(activity.resource_refs);
  const action = String(activity.action || "operation");
  return {
    id: activity.operation_id || "—",
    tool: action,
    args: resourceText(activity.resource_refs) || activity.purpose || "等待结果",
    addr: address,
    setsBp: action === "breakpoint_set" ? "ai" : null,
  };
}

function resourceText(value) {
  if (!value || typeof value !== "object") return value == null ? "" : String(value);
  if (Array.isArray(value)) return value.map(resourceText).filter(Boolean).join(", ");
  return Object.entries(value)
    .filter(([, item]) => item != null && item !== "")
    .map(([key, item]) => `${key}=${typeof item === "object" ? resourceText(item) : item}`)
    .join(" · ");
}

function addressFromValue(value) {
  const text = resourceText(value);
  const match = text.match(/0x[0-9a-f]+/i);
  return normalizeAddress(match?.[0]);
}

function fileName(path) {
  if (!path) return "";
  return String(path).split(/[\\/]/).filter(Boolean).pop() || String(path);
}

function formatCounter(value) {
  const text = String(value ?? "0");
  const number = Number(text);
  return Number.isSafeInteger(number) ? number.toLocaleString("en-US") : text;
}
