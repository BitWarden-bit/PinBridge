import React, { useCallback, useEffect, useState } from "react";
import Toolbar from "./components/Toolbar";
import DisasmView from "./components/DisasmView";
import Registers from "./components/Registers";
import StatsPanel from "./components/StatsPanel";
import BottomTabs from "./components/BottomTabs";
import LaunchScreen from "./components/LaunchScreen";
import { api } from "./api";
import { getSnapshot, subscribe } from "./store";
import { useT } from "./i18n";
import { clearResolveCache } from "./resolve";
import ModeBar from "./features/ai/ModeBar";
import AiWorkbench from "./features/ai/AiWorkbench";
import AiEmptyState from "./features/ai/AiEmptyState";
import { normalizeAddress } from "./address";
import UnifiedWorkbench from "./features/workspace/UnifiedWorkbench";

function formatCounter(value) {
  const text = String(value ?? "0");
  const number = Number(text);
  return Number.isSafeInteger(number) ? number.toLocaleString("en-US") : text;
}

export default function App() {
  return <UnifiedWorkbench />;
}

// Kept intact while the unified workspace is reviewed. The existing debugger
// backend and its current UI remain available for the integration phase.
export function LegacyDebuggerApp() {
  const t = useT();
  const [session, setSession] = useState({ running: false, target: null });
  const [status, setStatus] = useState({ text: "", err: false });
  const [tid, setTid] = useState(null);
  const [regs, setRegs] = useState([]);
  const [rows, setRows] = useState([]);
  const [stopTick, setStopTick] = useState(0);
  const [viewMode, setViewMode] = useState(() => localStorage.getItem("pb-view-mode") || "manual");
  const [control, setControl] = useState(null);
  const [controlServiceState, setControlServiceState] = useState("checking");
  const [actionBusy, setActionBusy] = useState(false);
  const [actionState, setActionState] = useState(null);
  const [, force] = React.useReducer((x) => x + 1, 0);
  const prevStopGen = React.useRef("0");

  useEffect(() => subscribe(force), []);
  useEffect(() => {
    const onErr = (e) => setStatus({ text: e.detail, err: true });
    window.addEventListener("pb-error", onErr);
    return () => window.removeEventListener("pb-error", onErr);
  }, []);
  useEffect(() => {
    api.session().then((s) => s && setSession(s));
  }, []);
  useEffect(() => {
    let cancelled = false;
    if (!session.running) {
      setControl(null);
      setControlServiceState("offline");
      return () => { cancelled = true; };
    }
    setControlServiceState("checking");
    api.ai.controlStatus().then((result) => {
      if (cancelled) return;
      if (result.ok && result.value?.mode) {
        setControl(result.value);
        setControlServiceState(result.value.ai_adapter_available === false ? "offline" : "connected");
      } else {
        setControl(null);
        setControlServiceState("offline");
      }
    });
    return () => { cancelled = true; };
  }, [session.running, session.target]);

  const loadContext = useCallback(async (targetTid) => {
    const useTid = targetTid ?? tid;
    if (useTid == null) return;
    const r = await api.context(useTid);
    if (r) setRegs(r);
  }, [tid]);

  const rip = (() => {
    const r = regs.find((x) => x.reg === 26);
    return normalizeAddress(r?.value) || "0x0";
  })();

  const loadDisasm = useCallback(async (addrText) => {
    // request exactly as many rows as fit the disasm panel (no scrolling)
    const panel = document.getElementById("disasm");
    const count = panel ? Math.max(8, Math.floor((panel.clientHeight - 34) / 19)) : 32;
    const data = await api.disasm(addrText, count);
    if (data && data.length) setRows(data);
  }, []);

  // Page up: rows ending just before the current top row (aligned decode
  // happens in the backend — never fabricate mid-instruction addresses here).
  const loadDisasmUp = useCallback(async (addrText) => {
    const panel = document.getElementById("disasm");
    const count = panel ? Math.max(8, Math.floor((panel.clientHeight - 34) / 19)) : 32;
    const data = await api.disasmUp(addrText, count);
    if (data && data.length) setRows(data);
  }, []);

  // Refresh disasm/registers/stack for the stopped state. tidHint is the
  // thread that hit a breakpoint (0xFFFFFFFF = pick the first stopped thread).
  const refreshStopped = useCallback(async (tidHint) => {
    const ids = await api.threads();
    if (ids && ids.length) {
      const pick = tidHint != null && tidHint !== 0xffffffff && ids.includes(tidHint) ? tidHint : ids[0];
      setTid(pick);
      const r = await api.context(pick);
      if (r) {
        setRegs(r);
        const ripEntry = r.find((x) => x.reg === 26);
        if (ripEntry) loadDisasm(normalizeAddress(ripEntry.value) || "0x0");
      }
    }
    setStopTick((x) => x + 1);
  }, [loadDisasm]);

  const followRip = useCallback(async () => {
    if (tid == null) return;
    const r = await api.context(tid);
    if (r) {
      setRegs(r);
      const ripVal = normalizeAddress(r.find((x) => x.reg === 26).value);
      if (ripVal) loadDisasm(ripVal);
    }
  }, [tid, loadDisasm]);

  const onSetBp = useCallback(async (address) => {
    await api.bpSet(address);
    setStatus({ text: t("breakpointAt") + address, err: false });
  }, []);

  const killSession = useCallback(async () => {
    await api.killBackend();
    clearResolveCache();
    setSession({ running: false, target: null });
    setRegs([]);
    setRows([]);
    setTid(null);
    setControl(null);
  }, []);

  const changeViewMode = useCallback((next) => {
    setViewMode(next);
    localStorage.setItem("pb-view-mode", next);
    setActionState(null);
  }, []);

  const handoffToAi = useCallback(async () => {
    if (actionBusy || !session.running) return;
    setActionBusy(true);
    setActionState({ kind: "pending", action: "handoff", message: t("handingOff") });
    const result = await api.ai.handoffToAi();
    setActionBusy(false);
    if (!result.ok) {
      setActionState({ kind: "error", message: `${t("handoffFailed")}: ${result.error}` });
      return;
    }
    const refreshed = await api.ai.controlStatus();
    if (refreshed.ok && refreshed.value?.mode) {
      setControl(refreshed.value);
      setControlServiceState("connected");
    } else {
      setControl(null);
      setControlServiceState("offline");
    }
    changeViewMode("ai");
    setActionState({ kind: "success", action: "handoff", message: t("handoffComplete") });
  }, [actionBusy, changeViewMode, session.running, t]);

  const takeoverManual = useCallback(async () => {
    if (actionBusy || !session.running) return;
    setActionBusy(true);
    setActionState({ kind: "pending", action: "takeover", message: t("takingOver") });
    const result = await api.ai.takeoverManual();
    setActionBusy(false);
    if (!result.ok) {
      setActionState({ kind: "error", message: `${t("takeoverFailed")}: ${result.error}` });
      return;
    }
    const refreshed = await api.ai.controlStatus();
    if (refreshed.ok && refreshed.value?.mode) {
      setControl(refreshed.value);
      setControlServiceState("connected");
    } else {
      setControl(null);
      setControlServiceState("offline");
    }
    changeViewMode("manual");
    setActionState({ kind: "success", action: "takeover", message: t("takeoverComplete") });
  }, [actionBusy, changeViewMode, session.running, t]);

  const snap = getSnapshot();
  // Every completed stop bumps the agent's stop generation counter. Keying
  // the refresh off the counter — not a running->stopped edge — catches
  // stop/run/stop cycles that complete inside one poll window (a single
  // step on a VMP target does exactly that, and the view would go stale).
  const stopGen = snap.connected ? String(snap.stopGen ?? "0") : "0";
  useEffect(() => {
    if (stopGen !== prevStopGen.current) {
      prevStopGen.current = stopGen;
      if (stopGen !== "0" && snap.stopped) {
        const hitTid = Number(snap.hitTid);
        refreshStopped(Number.isSafeInteger(hitTid) ? hitTid : null);
      }
    }
  }, [refreshStopped, snap.hitTid, snap.stopped, stopGen]);

  const bpSet = new Set(snap.bps.map((b) => normalizeAddress(b.address)).filter(Boolean));

  const modeBar = <ModeBar viewMode={viewMode} onModeChange={changeViewMode} control={control} serviceState={controlServiceState} onHandoff={handoffToAi} onTakeover={takeoverManual} actionBusy={actionBusy} actionState={actionState} hasSession={session.running} />;

  if (!session.running) {
    return (
      <>
        {modeBar}
        {viewMode === "ai" ? <AiEmptyState onManual={() => changeViewMode("manual")} /> : <LaunchScreen onLaunched={(target) => setSession({ running: true, target })} />}
        <div id="statusbar"><span style={{ color: "var(--dim)" }}>{t("noTarget")}</span></div>
      </>
    );
  }

  return (
    <>
      {modeBar}
      {viewMode === "ai" ? (
        <AiWorkbench snapshot={snap} session={session} control={control} onControl={setControl} onServiceState={setControlServiceState} onHandoff={handoffToAi} onTakeover={takeoverManual} actionBusy={actionBusy} />
      ) : (
        <>
          <Toolbar
            tid={tid}
            status={status}
            target={session.target}
            onKillSession={killSession}
            onStop={refreshStopped}
            onFollowRip={followRip}
            onGoto={loadDisasm}
          />
          <div id="main">
            <DisasmView rows={rows} rip={rip} bpSet={bpSet} onSetBp={onSetBp} onPage={loadDisasm} onPageUp={loadDisasmUp} />
            <div id="right">
              <Registers tid={tid} regs={regs} onChanged={() => loadContext(tid)} />
              <StatsPanel />
            </div>
          </div>
          <BottomTabs tid={tid} stopTick={stopTick} onGoto={loadDisasm} />
        </>
      )}
      <div id="statusbar">
        <span style={{ color: snap.connected ? "var(--fg)" : "var(--dim)" }}>
          {snap.connected ? "● " + t("connected") : "○ " + t("disconnected")}
        </span>
        {snap.connected && (
          <span style={{ color: snap.stopped ? "var(--err)" : "var(--fg)" }}>
            {snap.stopped ? "■ " + t("stopped") : "▶ " + t("running")}
            {snap.stopped && tid != null ? ` · Tid ${tid}` : ""}
            {snap.stopped && snap.hitAddr !== "0x0" ? ` @ ${snap.hitAddr}` : ""}
          </span>
        )}
        <span>Pid {snap.pid} · ABI {snap.abi[0]}.{snap.abi[1]}</span>
        <span>Total {formatCounter(snap.total)}</span>
        <span>Dropped {formatCounter(snap.dropped)}</span>
      </div>
    </>
  );
}
