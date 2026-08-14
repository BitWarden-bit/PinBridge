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

export default function App() {
  const t = useT();
  const [session, setSession] = useState({ running: false, target: null });
  const [status, setStatus] = useState({ text: "", err: false });
  const [tid, setTid] = useState(null);
  const [regs, setRegs] = useState([]);
  const [rows, setRows] = useState([]);
  const [stopTick, setStopTick] = useState(0);
  const [, force] = React.useReducer((x) => x + 1, 0);
  const prevStopGen = React.useRef(0);

  useEffect(() => subscribe(force), []);
  useEffect(() => {
    const onErr = (e) => setStatus({ text: e.detail, err: true });
    window.addEventListener("pb-error", onErr);
    return () => window.removeEventListener("pb-error", onErr);
  }, []);
  useEffect(() => {
    api.session().then((s) => s && setSession(s));
  }, []);

  const loadContext = useCallback(async (targetTid) => {
    const useTid = targetTid ?? tid;
    if (useTid == null) return;
    const r = await api.context(useTid);
    if (r) setRegs(r);
  }, [tid]);

  const rip = (() => {
    const r = regs.find((x) => x.reg === 26);
    return r ? parseInt(r.value, 16) : 0;
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
        if (ripEntry) loadDisasm("0x" + parseInt(ripEntry.value, 16).toString(16));
      }
    }
    setStopTick((x) => x + 1);
  }, [loadDisasm]);

  const followRip = useCallback(async () => {
    if (tid == null) return;
    const r = await api.context(tid);
    if (r) {
      setRegs(r);
      const ripVal = parseInt(r.find((x) => x.reg === 26).value, 16);
      loadDisasm("0x" + ripVal.toString(16));
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
  }, []);

  const snap = getSnapshot();
  // Every completed stop bumps the agent's stop generation counter. Keying
  // the refresh off the counter — not a running->stopped edge — catches
  // stop/run/stop cycles that complete inside one poll window (a single
  // step on a VMP target does exactly that, and the view would go stale).
  const stopGen = snap.connected ? snap.stopGen : 0;
  useEffect(() => {
    if (stopGen !== prevStopGen.current) {
      prevStopGen.current = stopGen;
      if (stopGen > 0 && snap.stopped) refreshStopped(snap.hitTid);
    }
  });

  const bpSet = new Set(snap.bps.map((b) => b.address));

  if (!session.running) {
    return (
      <>
        <LaunchScreen onLaunched={(target) => setSession({ running: true, target })} />
        <div id="statusbar"><span style={{ color: "var(--dim)" }}>{t("noTarget")}</span></div>
      </>
    );
  }

  return (
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
        <span>Total {snap.total.toLocaleString("en-US")}</span>
        <span>Dropped {snap.dropped.toLocaleString("en-US")}</span>
      </div>
    </>
  );
}
