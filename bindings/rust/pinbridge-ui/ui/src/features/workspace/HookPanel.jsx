import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { api } from "../../api";
import { normalizeAddress } from "../../address";
import { RvaRangeEditor, ValueTokenEditor } from "../../components/StructuredInputs";
import CallbackEditorDialog from "./CallbackEditorDialog";

const HOOK_PAGE_SIZE = 300;
const EXPORT_PAGE_SIZE = 400;

export default function HookPanel({ rip, stopped, onGoto, stopTick, activities = [] }) {
  const [mode, setMode] = useState("syscall");
  const [inventoryPage, setInventoryPage] = useState(0);
  const [inventory, setInventory] = useState({ hooks: [], count: "0", capacity: "32768" });
  const [hookAddresses, setHookAddresses] = useState([]);
  const [functionAddresses, setFunctionAddresses] = useState([]);
  const [events, setEvents] = useState([]);
  const [monitorStats, setMonitorStats] = useState({ lane_dropped: "0", history_overwritten: "0" });
  const [syscallEvents, setSyscallEvents] = useState([]);
  const [syscallStats, setSyscallStats] = useState({ ring_dropped: "0", history_overwritten: "0" });
  const [modules, setModules] = useState([]);
  const [scripts, setScripts] = useState([]);
  const [output, setOutput] = useState([]);
  const outputCursor = useRef("0");
  const [selectedAddress, setSelectedAddress] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const refreshInventory = useCallback(async () => {
    const inventoryKind = mode === "api" ? "api" : "instruction";
    const wantsInventory = mode === "api" || mode === "agent";
    const wantsHookList = mode === "api";
    const wantsModules = mode === "api" || mode === "syscall";
    const wantsScripts = mode === "api" || mode === "agent";
    const [inventoryResult, listResult, modulesResult, scriptsResult] = await Promise.all([
      wantsInventory ? api.hookInventory(inventoryPage * HOOK_PAGE_SIZE, HOOK_PAGE_SIZE, inventoryKind) : null,
      wantsHookList ? api.hookList() : null,
      wantsModules ? api.modules() : null,
      wantsScripts ? api.scriptList() : null,
    ]);
    const errors = [];
    if (inventoryResult?.ok) setInventory(inventoryResult.value || { hooks: [] });
    else if (inventoryResult) errors.push(inventoryResult.error);
    if (listResult?.ok) {
      const rows = Array.isArray(listResult.value?.hooks) ? listResult.value.hooks : [];
      setHookAddresses(rows.map((row) => normalizeAddress(row.address)).filter(Boolean));
      setFunctionAddresses(rows.filter((row) => row.function_log).map((row) => normalizeAddress(row.address)).filter(Boolean));
    } else if (listResult) errors.push(listResult.error);
    if (Array.isArray(modulesResult)) setModules(modulesResult);
    if (scriptsResult?.ok) setScripts(Array.isArray(scriptsResult.value) ? scriptsResult.value : []);
    else if (scriptsResult) errors.push(scriptsResult.error);
    setError(errors.filter(Boolean).join(" · "));
  }, [inventoryPage, mode]);

  const refreshLive = useCallback(async () => {
    const hookLimit = mode === "logs" ? 512 : mode === "api" || mode === "agent" ? 256 : 0;
    const syscallLimit = mode === "logs" ? 512 : 0;
    const wantsOutput = mode === "api" || mode === "agent";
    const [monitorResult, syscallResult, outputResult] = await Promise.all([
      hookLimit ? api.hookEventsQuery({ limit: String(hookLimit), layout: "events", order: "asc" }) : null,
      syscallLimit ? api.syscallMonitor(syscallLimit) : null,
      wantsOutput ? api.scriptOutput(outputCursor.current, "256") : null,
    ]);
    const errors = [];
    if (monitorResult?.ok) {
      setEvents(Array.isArray(monitorResult.value?.events) ? monitorResult.value.events : []);
      setMonitorStats({ ...(monitorResult.value?.lane || {}), ...(monitorResult.value || {}) });
    }
    else if (monitorResult) errors.push(monitorResult.error);
    if (syscallResult?.ok) {
      setSyscallEvents(Array.isArray(syscallResult.value?.events) ? syscallResult.value.events : []);
      setSyscallStats(syscallResult.value || { ring_dropped: "0", history_overwritten: "0" });
    }
    else if (syscallResult) errors.push(syscallResult.error);
    if (outputResult?.ok) {
      const lines = Array.isArray(outputResult.value?.lines) ? outputResult.value.lines : [];
      outputCursor.current = String(outputResult.value?.next_cursor || outputCursor.current);
      if (lines.length) setOutput((current) => [...current, ...lines].slice(-2048));
    } else if (outputResult) errors.push(outputResult.error);
    setError(errors.filter(Boolean).join(" · "));
  }, [mode]);

  const refresh = useCallback(async () => {
    await Promise.all([refreshInventory(), refreshLive()]);
  }, [refreshInventory, refreshLive]);

  useEffect(() => {
    let live = true;
    let timer = 0;
    const tick = async () => {
      if (!live) return;
      await refreshLive();
      if (live) timer = window.setTimeout(tick, 1600);
    };
    const initial = async () => {
      await refresh();
      if (live) timer = window.setTimeout(tick, 1600);
    };
    initial();
    return () => { live = false; window.clearTimeout(timer); };
  }, [refresh, refreshLive, stopTick]);

  const hooks = Array.isArray(inventory?.hooks) ? inventory.hooks : [];
  const selected = mode === "api" || mode === "agent"
    ? hooks.find((hook) => normalizeAddress(hook.address) === selectedAddress) || null
    : null;

  async function mutate(action) {
    if (busy) return null;
    setBusy(true);
    const result = await action();
    setBusy(false);
    if (!result?.ok) {
      setError(result?.error || "Hook 操作失败");
      return null;
    }
    setError("");
    await refreshInventory();
    return result.value;
  }

  return (
    <section className="pba-panel pbh-panel">
      <header className="pba-panel-head">
        <div><b>Hook</b><span>监控点 {hookAddresses.length} / {inventory?.capacity || "32768"} · 只有回调 Hook 可修改上下文</span></div>
        <button onClick={refresh} disabled={busy}>刷新</button>
      </header>
      <nav className="pbe-mode-tabs" aria-label="Hook 类型">
        <button className={mode === "syscall" ? "active" : ""} onClick={() => { setMode("syscall"); setInventoryPage(0); setSelectedAddress(""); }}>系统调用</button>
        <button className={mode === "agent" ? "active" : ""} onClick={() => { setMode("agent"); setInventoryPage(0); setSelectedAddress(""); }}>回调 Hook</button>
        <button className={mode === "api" ? "active" : ""} onClick={() => { setMode("api"); setInventoryPage(0); setSelectedAddress(""); }}>监控 Hook</button>
        <button className={mode === "logs" ? "active" : ""} onClick={() => { setMode("logs"); setInventoryPage(0); setSelectedAddress(""); }}>事件</button>
      </nav>
      {error && <div className="pba-error pbe-global-error" role="alert">{error}</div>}
      {mode === "logs" ? (
        <HookLogView
          latestEvents={events}
          latestStats={monitorStats}
          latestSyscallEvents={syscallEvents}
          latestSyscallStats={syscallStats}
          onGoto={onGoto}
          onRefresh={refreshLive}
        />
      ) : mode === "syscall" ? (
          <SyscallHooks modules={modules} />
      ) : selected ? (
          <HookDetail
            hook={selected}
            events={events}
            output={output}
            busy={busy}
            onBack={() => setSelectedAddress("")}
            onGoto={onGoto}
            onMutate={mutate}
            onRefresh={refreshInventory}
            onError={setError}
            pointerWidth={monitorStats?.pointer_width}
          />
      ) : mode === "agent" ? (
          <CallbackHookManager
            hooks={hooks}
            events={events}
            rip={rip}
            stopped={stopped}
            busy={busy}
            onGoto={onGoto}
            onSelectHook={setSelectedAddress}
            onMutate={mutate}
            page={inventoryPage}
            total={Number(inventory?.count || 0)}
            onPage={setInventoryPage}
          />
      ) : (
        <DllHooks
          modules={modules}
          functionAddresses={functionAddresses}
          busy={busy}
          onGoto={onGoto}
          onSelectHook={(address) => {
            const normalized = normalizeAddress(address);
            const index = functionAddresses.indexOf(normalized);
            if (index >= 0) setInventoryPage(Math.floor(index / HOOK_PAGE_SIZE));
            setSelectedAddress(normalized);
            setMode("api");
          }}
          onMutate={mutate}
          onError={setError}
        />
      )}
    </section>
  );
}

function CallbackHookManager({ hooks, events, rip, stopped, busy, onGoto, onSelectHook, onMutate, page, total, onPage }) {
  const [address, setAddress] = useState("");
  const [ownerFilter, setOwnerFilter] = useState("all");
  const [search, setSearch] = useState("");
  const current = normalizeAddress(rip);
  const canUseCurrent = stopped && current && current !== "0x0";
  const hitCounts = useMemo(() => countHits(events), [events]);
  const pageCount = Math.max(1, Math.ceil(total / HOOK_PAGE_SIZE));
  const safePage = Math.min(page, pageCount - 1);
  const visibleHooks = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return hooks.filter((hook) => {
      const actors = hookOwnerActors(hook);
      if (ownerFilter === "human" && !actors.includes("human")) return false;
      if (ownerFilter === "ai" && !actors.includes("ai")) return false;
      if (ownerFilter === "shared" && !(actors.includes("human") && actors.includes("ai"))) return false;
      if (!needle) return true;
      const callbacks = Array.isArray(hook.callbacks) ? hook.callbacks : [];
      return [
        hook.address,
        hook.display,
        hook.module,
        hook.symbol,
        ownerText(hook),
        ...callbacks.flatMap((callback) => [callback.plugin, callback.callback, callback.description]),
      ].filter(Boolean).some((value) => String(value).toLowerCase().includes(needle));
    });
  }, [hooks, ownerFilter, search]);

  useEffect(() => {
    if (page !== safePage) onPage(safePage);
  }, [page, safePage, onPage]);

  async function add(target) {
    const normalized = normalizeAddress(target);
    if (!normalized || busy) return;
    const value = await onMutate(() => api.hookSet(normalized));
    if (!value) return;
    setAddress("");
    onSelectHook(normalized);
  }

  return (
    <div className="pbh-body pbh-callback-manager">
      <div className="pbh-callback-summary">
        <div><b>Hook 回调管理</b><span>普通监控与同步回调共用同一地址清单</span></div>
        <div className="pbh-owner-legend"><span><i className="human" />人工</span><span><i className="ai" />AI</span><span><i className="human" /><i className="ai" />共享位置</span></div>
      </div>
      <div className="pba-add-row pbh-callback-add">
        <input
          value={address}
          placeholder={canUseCurrent ? current : "0x Hook 地址"}
          spellCheck="false"
          onChange={(event) => setAddress(event.target.value)}
          onKeyDown={(event) => event.key === "Enter" && add(address)}
        />
        <button disabled={busy || !normalizeAddress(address)} onClick={() => add(address)}>添加 Hook</button>
        <button className="primary" disabled={busy || !canUseCurrent} onClick={() => add(current)}>＋ 当前地址</button>
      </div>
      <div className="pbh-callback-toolbar">
        <div className="pbh-callback-filters">
          {[["all", "全部"], ["human", "人工"], ["ai", "AI"], ["shared", "共享"]].map(([value, label]) => (
            <button key={value} className={ownerFilter === value ? "active" : ""} onClick={() => setOwnerFilter(value)}>{label}</button>
          ))}
        </div>
        <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="筛选地址、符号、脚本或回调" />
        <span>本页 {visibleHooks.length} / {hooks.length}</span>
      </div>
      <div className="pbh-callback-head"><span>Hook 位置 / 符号</span><span>归属</span><span>回调</span><span>最近状态</span><span /></div>
      <div className="pba-list pbh-callback-list">
        {visibleHooks.length === 0 && <div className="pba-empty"><b>无匹配 Hook</b><span>添加地址或调整归属筛选</span></div>}
        {visibleHooks.map((hook) => (
          <CallbackHookRow
            key={normalizeAddress(hook.address) || hook.address}
            hook={hook}
            hits={hitCounts.get(normalizeAddress(hook.address)) || 0}
            onGoto={onGoto}
            onSelect={onSelectHook}
          />
        ))}
      </div>
      <HookPager page={safePage} pageCount={pageCount} total={total} pageSize={HOOK_PAGE_SIZE} onPage={onPage} />
    </div>
  );
}

function CallbackHookRow({ hook, hits, onGoto, onSelect }) {
  const address = normalizeAddress(hook.address) || hook.address;
  const callbacks = Array.isArray(hook.callbacks) ? hook.callbacks : [];
  const actors = hookOwnerActors(hook);
  const latest = latestHookCallback(callbacks);
  const state = latest ? callbackRuntimeState(latest) : { label: hits ? `命中 ${hits}` : "等待命中", tone: "" };
  return (
    <div className="pbh-callback-row" role="button" tabIndex={0} onClick={() => onSelect(address)} onKeyDown={(event) => event.key === "Enter" && onSelect(address)}>
      <span className="pbh-hook-address"><code>{address}</code><small>{hook.display || symbolText(hook)}</small></span>
      <HookOwnerBadges actors={actors} />
      <span className="pbh-callback-count"><b>{callbacks.length ? `${callbacks.length} 个` : "无"}</b><small>{callbacks.length ? callbacks.map((callback) => callback.callback).join(" · ") : "原生监控"}</small></span>
      <em className={state.tone}>{state.label}</em>
      <span className="pbh-row-actions"><button onClick={(event) => { event.stopPropagation(); onGoto(address); }}>定位</button><button className="primary" onClick={(event) => { event.stopPropagation(); onSelect(address); }}>详情</button></span>
    </div>
  );
}

function HookOwnerBadges({ actors }) {
  if (!actors.length) return <span className="pbh-owner-badges"><em className="external"><i />外部</em></span>;
  return <span className="pbh-owner-badges">{actors.map((actor) => <em key={actor} className={actor}><i />{actorLabel(actor)}</em>)}</span>;
}

const HOOK_LOG_PAGE_SIZE = 2048;

function HookLogView({ latestEvents, latestStats, latestSyscallEvents, latestSyscallStats, onGoto, onRefresh }) {
  const [before, setBefore] = useState("0");
  const [page, setPage] = useState(null);
  const [pageStats, setPageStats] = useState(null);
  const [history, setHistory] = useState([]);
  const [kindFilter, setKindFilter] = useState("all");
  const [phaseFilter, setPhaseFilter] = useState("all");
  const [search, setSearch] = useState("");
  const [order, setOrder] = useState("desc");
  const [exportLayout, setExportLayout] = useState("events");
  const [exportFormat, setExportFormat] = useState("jsonl");
  const [exporting, setExporting] = useState(false);
  const [loading, setLoading] = useState(false);
  const [pageError, setPageError] = useState("");

  const latestUnifiedEvents = useMemo(
    () => [...latestEvents, ...latestSyscallEvents],
    [latestEvents, latestSyscallEvents],
  );
  const sourceEvents = before === "0" ? latestUnifiedEvents : (page || []);
  const sourceHookEvents = before === "0" ? latestEvents : (page || []);
  const stats = before === "0" ? latestStats : (pageStats || latestStats);
  const paired = useMemo(() => pairHookEvents(sourceEvents), [sourceEvents]);
  const apiCount = paired.filter((event) => hookEventType(event) === "api").length;
  const instructionCount = paired.filter((event) => hookEventType(event) === "instruction").length;
  const syscallCount = paired.filter((event) => hookEventType(event) === "syscall").length;
  const visible = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const filtered = paired.filter((event) => {
      const type = hookEventType(event);
      if (kindFilter !== "all" && type !== kindFilter) return false;
      if (phaseFilter !== "all" && event.kind !== phaseFilter) return false;
      if (!needle) return true;
      return [event.display, event.symbol, event.module, event.address, event.thread_id, event.number, event.generation, hookEventPayload(event)]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(needle));
    });
    filtered.sort((left, right) => compareHookEvents(left, right) * (order === "asc" ? 1 : -1));
    return filtered;
  }, [paired, kindFilter, phaseFilter, search, order]);

  async function loadWindow(cursor, pushHistory) {
    if (loading) return;
    setLoading(true);
    const result = await api.hookEventsQuery({
      limit: String(HOOK_LOG_PAGE_SIZE),
      before: String(cursor || "0"),
      layout: "events",
      order: "asc",
    });
    setLoading(false);
    if (!result.ok) {
      setPageError(result.error);
      return;
    }
    const rows = Array.isArray(result.value?.events) ? result.value.events : [];
    if (pushHistory && rows.length === 0) {
      setPageError("已到日志起点");
      return;
    }
    if (pushHistory) setHistory((current) => [...current, { before, page, pageStats }]);
    setBefore(String(cursor || "0"));
    setPage(rows);
    setPageStats({ ...(result.value?.lane || {}), ...(result.value || {}) });
    setPageError("");
  }

  async function older() {
    const first = minimumHookSequence(sourceHookEvents);
    if (first) await loadWindow(String(first), true);
  }

  function newer() {
    const previous = history[history.length - 1];
    if (!previous) {
      setBefore("0");
      setPage(null);
      setPageStats(null);
      return;
    }
    setHistory((current) => current.slice(0, -1));
    setBefore(previous.before);
    setPage(previous.page);
    setPageStats(previous.pageStats);
    setPageError("");
  }

  function newest() {
    setBefore("0");
    setPage(null);
    setPageStats(null);
    setHistory([]);
    setPageError("");
    onRefresh();
  }

  async function exportHookEvents() {
    if (exporting || kindFilter === "syscall") return;
    setExporting(true);
    const query = {
      limit: "4096",
      order,
      layout: exportLayout,
      format: exportFormat,
      filename: `hook-events-${exportLayout}.${exportFormat}`,
    };
    if (before !== "0") query.before = before;
    if (kindFilter === "api" || kindFilter === "instruction") query.hook_types = [kindFilter];
    if (phaseFilter !== "all") query.phases = [phaseFilter];
    const result = await api.hookEventsExport(query);
    setExporting(false);
    if (!result.ok) {
      setPageError(result.error);
      return;
    }
    const value = result.value || {};
    const blob = new Blob([String(value.data || "")], { type: value.mime_type || "text/plain" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = value.filename || query.filename;
    link.click();
    URL.revokeObjectURL(url);
    setPageError("");
  }

  const chronological = [...sourceEvents].sort(compareHookEvents);
  const hookChronological = [...sourceHookEvents].sort((left, right) => hookSequenceOrder(left, right));
  const syscallChronological = sourceEvents.filter((event) => hookEventType(event) === "syscall").sort((left, right) => hookSequenceOrder(left, right));
  const firstTime = chronological[0] ? formatHookTime(chronological[0]) : "—";
  const lastTime = chronological.length ? formatHookTime(chronological[chronological.length - 1]) : "—";
  return <div className="pbh-body pbh-log-view">
    <div className="pbh-log-summary">
      <div><b>事件</b><span>{firstTime} — {lastTime} · {sourceEvents.length} 条{before === "0" ? " · 最新窗口" : " · Hook 历史"}</span></div>
      <div><em className="api">API {apiCount}</em><em>指令 {instructionCount}</em><em className="syscall">系统调用 {syscallCount}</em><em className={Number(stats?.lane_dropped || 0) || Number(latestSyscallStats?.ring_dropped || 0) ? "warn" : ""}>丢弃 H {stats?.lane_dropped || "0"} / S {latestSyscallStats?.ring_dropped || "0"}</em></div>
    </div>
    <div className="pbh-log-toolbar">
      <div className="pbh-log-filters">
        {[["all", "全部"], ["api", "API"], ["instruction", "指令"], ["syscall", "系统调用"]].map(([value, label]) => <button key={value} className={kindFilter === value ? "active" : ""} onClick={() => setKindFilter(value)}>{label}</button>)}
      </div>
      <select value={phaseFilter} onChange={(event) => setPhaseFilter(event.target.value)}><option value="all">全部阶段</option><option value="hit">指令命中</option><option value="entry">入口</option><option value="return">返回 / 出口</option></select>
      <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="筛选目标、编号、地址或线程" />
      <button onClick={() => setOrder((value) => value === "desc" ? "asc" : "desc")}>时间 {order === "desc" ? "↓ 新到旧" : "↑ 旧到新"}</button>
      <select value={exportLayout} onChange={(event) => setExportLayout(event.target.value)}><option value="events">原始事件</option><option value="calls">配对调用</option><option value="summary">聚合摘要</option></select>
      <select value={exportFormat} onChange={(event) => setExportFormat(event.target.value)}><option value="jsonl">JSONL</option><option value="json">JSON</option><option value="csv">CSV</option></select>
      <button disabled={exporting || kindFilter === "syscall"} title={kindFilter === "syscall" ? "系统调用使用独立导出通道" : "按当前 Hook 类型和阶段导出"} onClick={exportHookEvents}>{exporting ? "导出中…" : "导出 Hook"}</button>
    </div>
    {pageError && <div className="pba-error">{pageError}</div>}
    <div className="pbh-log-head"><span>时间</span><span>类型</span><span>目标</span><span>线程 / 配对</span><span>参数 / 返回值</span><span /></div>
    <div className="pba-list pbh-log-list">
      {visible.length === 0 && <div className="pba-empty"><b>无匹配日志</b><span>调整筛选条件</span></div>}
      {visible.map((event) => <HookLogRow key={`${hookEventType(event)}:${event.sequence}`} event={event} pointerWidth={stats?.pointer_width} onGoto={onGoto} />)}
    </div>
    <div className="pbh-log-pager">
      <span>Hook {sequenceRange(hookChronological)}{before === "0" && <> · Syscall {sequenceRange(syscallChronological)}</>} · 显示 {visible.length} 条 · 覆盖 H {stats?.history_overwritten || "0"}{before === "0" && <> / S {latestSyscallStats?.history_overwritten || "0"}</>}</span>
      <div><button disabled={loading || !sourceHookEvents.length} onClick={older}>更早 Hook</button><button disabled={loading || (before === "0" && history.length === 0)} onClick={newer}>较新一页</button><button className="primary" disabled={loading || before === "0"} onClick={newest}>回到最新</button></div>
    </div>
  </div>;
}

function HookLogRow({ event, pointerWidth, onGoto }) {
  const [expanded, setExpanded] = useState(false);
  const type = hookEventType(event);
  const isReturn = event.kind === "return";
  const functionName = type === "syscall"
    ? `Syscall ${event.number || "—"}`
    : type === "api"
      ? event.display || event.symbol || event.signature?.function || event.address
      : event.address;
  const secondary = type === "syscall" ? `generation ${event.generation || "0"}` : type === "api" ? event.address : event.display || "指令地址";
  const pairText = isReturn
    ? (event.call_sequence ? `入口 #${event.call_sequence}` : "入口不在本页")
    : (event.return_sequence ? `返回 #${event.return_sequence}` : type === "instruction" ? "单点命中" : "等待返回");
  return <div className={`pbh-log-record ${expanded ? "expanded" : ""}`}>
    <div className={`pbh-log-row ${type} ${isReturn ? "return" : "entry"}`}>
      <time title={formatHookTime(event, true)}>{formatHookTime(event)}</time>
      <span><b>{type === "syscall" ? "系统调用" : type === "api" ? "API Hook" : "指令 Hook"}</b><small>{type === "instruction" ? "命中" : isReturn ? (type === "syscall" ? "出口" : "返回") : "入口"}</small></span>
      <span className="pbh-log-target"><b>{functionName}</b><code>{secondary}</code></span>
      <span><b>TID {event.thread_id}</b><small>{pairText}</small></span>
      <code className="pbh-log-payload" title={hookEventPayload(event)}>{hookEventPayload(event)}</code>
      <button aria-expanded={expanded} onClick={() => setExpanded((value) => !value)}>{expanded ? "收起" : "展开"}</button>
    </div>
    {expanded && <HookEventContext event={event} pointerWidth={pointerWidth} onGoto={onGoto} />}
  </div>;
}

function AgentInstructionRules({ hooks, events, scripts, output, activities, onGoto, onRefresh, page, total, onPage }) {
  const callbackScripts = useMemo(
    () => {
      const boundPlugins = new Set(hooks.flatMap((hook) => (hook.callbacks || []).map((callback) => callback.plugin)));
      return scripts.filter((script) => (
        boundPlugins.has(script.name) || script.created_by === "ai" || script.modified_by === "ai"
      ));
    },
    [hooks, scripts],
  );
  const [selectedName, setSelectedName] = useState("");
  const [source, setSource] = useState("");
  const [sourceMeta, setSourceMeta] = useState(null);
  const [sourceError, setSourceError] = useState("");
  const [loading, setLoading] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const hitCounts = useMemo(() => countHits(events), [events]);
  const pageCount = Math.max(1, Math.ceil(total / HOOK_PAGE_SIZE));
  const safePage = Math.min(page, pageCount - 1);
  const callbackHooks = hooks.filter(isCallbackHook);
  const selected = callbackScripts.find((script) => script.name === selectedName) || callbackScripts[0] || null;
  const selectedBindings = callbackHooks.flatMap((hook) => (hook.callbacks || [])
    .filter((callback) => callback.plugin === selected?.name)
    .map((callback) => ({ hook, callback })));
  const pluginOutput = output.filter((line) => line.plugin === selected?.name).slice(-120);
  const relevantActivities = useMemo(() => {
    const plugin = String(selected?.name || "").toLowerCase();
    return (Array.isArray(activities) ? activities : [])
      .filter((activity) => {
        if (String(activity.actor || "").toLowerCase() !== "ai") return false;
        const action = String(activity.action || "").toLowerCase();
        const resources = hookActivityResourceText(activity.resource_refs).toLowerCase();
        return /hook|script|callback|intercept/.test(action) || Boolean(plugin && resources.includes(plugin));
      })
      .slice(0, 8);
  }, [activities, selected?.name]);
  const callbackCount = callbackHooks.reduce((count, hook) => count + (hook.callbacks || []).length, 0);
  const selectedHits = selectedBindings.reduce((count, item) => count + (hitCounts.get(normalizeAddress(item.hook.address)) || 0), 0);
  const selectedErrors = selectedBindings.filter((item) => item.callback.last_error).length;
  const sourceLines = source ? source.split(/\r?\n/).length : 0;

  useEffect(() => {
    if (page !== safePage) onPage(safePage);
  }, [page, safePage, onPage]);

  useEffect(() => {
    if (!selected) {
      setSource("");
      setSourceMeta(null);
      setSourceError("");
      return;
    }
    let live = true;
    setLoading(true);
    api.scriptGet(selected.name).then((result) => {
      if (!live) return;
      setLoading(false);
      if (!result.ok) {
        setSource("");
        setSourceMeta(null);
        setSourceError(result.error);
        return;
      }
      setSource(String(result.value?.source || ""));
      setSourceMeta(result.value || null);
      setSourceError("");
    });
    return () => { live = false; };
  }, [selected?.name]);

  async function saveSource(draft) {
    if (!selected || saving) return false;
    const name = String(draft?.name || "").trim();
    const nextSource = String(draft?.source || "");
    if (name !== selected.name || !nextSource.trim()) return false;
    setSaving(true);
    setSourceError("");
    const result = await api.scriptReplace(name, nextSource, selected.kind || "callback");
    setSaving(false);
    if (!result.ok) {
      setSourceError(result.error);
      return false;
    }
    setSource(nextSource);
    setSourceMeta(result.value || sourceMeta);
    setEditorOpen(false);
    await onRefresh?.();
    return true;
  }

  return (
    <div className="pbh-body pbh-agent-rules">
      <section className="pbh-agent-hero">
        <div className="pbh-agent-hero-copy">
          <span>AI HOOK WORKSPACE</span>
          <b>AI 正在管理的回调策略</b>
          <p>聚焦策略、绑定、动作和结果。源码不占用监控空间，统一进入独立代码编辑器。</p>
        </div>
        <div className="pbh-agent-hero-stats">
          <div><span>策略</span><b>{callbackScripts.length}</b></div>
          <div><span>回调点</span><b>{callbackHooks.length}</b></div>
          <div><span>绑定</span><b>{callbackCount}</b></div>
          <div className={selectedErrors ? "warn" : ""}><span>错误</span><b>{selectedErrors}</b></div>
        </div>
      </section>

      <div className="pbh-agent-workspace">
        <section className="pbh-agent-script-list">
          <div className="pbh-installed-title"><div><b>AI 策略</b><span>{callbackScripts.length}</span></div><span>脚本资产</span></div>
          <div className="pba-list">
            {callbackScripts.length === 0 && <div className="pba-empty"><b>暂无回调脚本</b></div>}
            {callbackScripts.map((script) => <button key={script.name} className={`pbh-agent-script-row ${selected?.name === script.name ? "active" : ""}`} onClick={() => setSelectedName(script.name)}>
              <i className={`pba-dot ${script.created_by === "ai" || script.modified_by === "ai" ? "ai" : "strategy"}`} />
              <span><b>{script.name}</b><small>{scriptOriginLabel(script)} · generation {script.generation || "0"}</small></span>
              <em className={isLoadedScript(script) ? "armed" : ""}>{script.state}</em>
            </button>)}
          </div>
        </section>
        <section className="pbh-agent-focus">
          {!selected ? <div className="pba-empty"><b>选择一个 AI 策略</b><span>查看它安装的 Hook、最近动作和运行结果</span></div> : <>
            <header className="pbh-agent-focus-head">
              <div><span>当前策略</span><b>{selected.name}</b><small>{scriptOriginLabel(selected)} · generation {selected.generation || "0"}</small></div>
              <div><em className={isLoadedScript(selected) ? "armed" : ""}>{isLoadedScript(selected) ? "已装载" : selected.state}</em><code>{sourceMeta?.source_hash ? sourceMeta.source_hash.slice(0, 12) : "source pending"}</code></div>
            </header>
            <div className="pbh-agent-kpis">
              <div><span>同步绑定</span><b>{selectedBindings.length}</b><small>可检查或修改上下文</small></div>
              <div><span>窗口命中</span><b>{selectedHits}</b><small>当前 Hook 日志窗口</small></div>
              <div className={selectedErrors ? "warn" : ""}><span>回调错误</span><b>{selectedErrors}</b><small>{selectedErrors ? "需要检查返回或异常" : "未发现错误"}</small></div>
            </div>
            <section className="pbh-agent-activity">
              <div className="pbh-agent-section-title"><div><b>最近 AI 动作</b><span>只显示 Hook / Script 相关操作</span></div><span>{relevantActivities.length} 条</span></div>
              <div className="pbh-agent-activity-list">
                {relevantActivities.length === 0 && <div className="pba-empty"><b>暂无 Hook 相关 AI 活动</b></div>}
                {relevantActivities.map((activity) => <HookActivityRow key={activity.operation_id || activity.started_at_ms} activity={activity} />)}
              </div>
            </section>
          </>}
        </section>
      </div>

      {selected && <section className="pbh-agent-bindings">
        <div className="pbh-agent-section-title"><div><b>Hook 绑定</b><span>AI 策略实际接管的地址与回调函数</span></div><span>{selectedBindings.length} 个</span></div>
        <div className="pbh-agent-binding-head"><span>地址 / 符号</span><span>回调 / 目的</span><span>事件</span><span>模式</span><span>状态</span><span /></div>
        <div className="pbh-agent-binding-list">
          {selectedBindings.length === 0 && <div className="pba-empty"><b>当前页无同步回调绑定</b></div>}
          {selectedBindings.map(({ hook, callback }) => {
            const state = callbackRuntimeState(callback);
            const address = normalizeAddress(hook.address) || hook.address;
            return <div className="pbh-agent-binding-row" key={`${hook.address}:${callback.id}`}>
              <span><code>{address}</code><small>{hook.display || symbolText(hook)}</small></span>
              <span><b>{callback.callback}</b><small>{callback.description || "未提供回调说明"}</small></span>
              <code>{callback.selector}</code>
              <span>{callback.once ? "一次性" : "持续"}</span>
              <em className={state.tone}>{state.label}</em>
              <button onClick={() => onGoto(address)}>定位</button>
            </div>;
          })}
        </div>
      </section>}

      <div className="pbh-agent-lower">
        <section className="pbh-agent-output">
          <div className="pbh-agent-section-title"><div><b>运行输出</b><span>AI 策略加载、注册、命中与错误</span></div><span>{pluginOutput.length} 行</span></div>
          <pre>{pluginOutput.length ? pluginOutput.map((line) => `[${line.seq}] ${line.line}`).join("\n") : "暂无策略输出"}</pre>
        </section>
        <section className="pbh-agent-code-card">
          <div className="pbh-agent-section-title"><div><b>代码资产</b><span>源码在独立编辑器中查看与修改</span></div><span>Python</span></div>
          {sourceError && <div className="pba-error">{sourceError}</div>}
          <div className="pbh-agent-code-file">
            <i>PY</i>
            <div><b>{selected?.name || "未选择脚本"}</b><span>{loading ? "读取源码…" : sourceMeta ? `${sourceLines} 行 · generation ${sourceMeta.generation || selected?.generation || "0"}` : "源码不可用"}</span></div>
            <button className="primary" disabled={!selected || loading || !sourceMeta} onClick={() => setEditorOpen(true)}>打开代码编辑器</button>
          </div>
          <p>监控页只展示 AI 的策略状态和运行行为；代码审阅、修改与 API 索引统一在编辑器完成。</p>
        </section>
      </div>

      <HookPager
        page={safePage}
        pageCount={pageCount}
        total={total}
        pageSize={HOOK_PAGE_SIZE}
        onPage={onPage}
      />
      <CallbackEditorDialog
        open={editorOpen}
        creating={false}
        name={selected?.name || ""}
        source={source}
        meta={sourceMeta}
        error={sourceError}
        loading={loading}
        saving={saving}
        readOnly={!sourceMeta}
        callbackKind="Hook 策略"
        onClose={() => setEditorOpen(false)}
        onApply={saveSource}
      />
    </div>
  );
}

function HookActivityRow({ activity }) {
  const inFlight = !activity.completed_at_ms && activity.outcome === "in_progress";
  const tone = inFlight ? "wait" : activity.outcome === "ok" ? "ok" : activity.outcome ? "err" : "";
  return <div className="pbh-agent-activity-row">
    <time>{formatHookActivityTime(activity.started_at_ms)}</time>
    <code>{activity.action || "operation"}</code>
    <span title={activity.purpose || ""}>{activity.purpose || hookActivityResourceText(activity.resource_refs) || "未提供目的"}</span>
    <em className={tone}>{inFlight ? "进行中" : activity.outcome || "—"}</em>
  </div>;
}

function callbackRuntimeState(callback) {
  if (callback.last_error) return { label: "回调错误", tone: "err" };
  if (Number(callback.last_generation || 0) > 0) return { label: `已执行 · gen ${callback.last_generation}`, tone: "ok" };
  return { label: "等待命中", tone: "wait" };
}

function formatHookActivityTime(value) {
  if (!/^\d+$/.test(String(value || ""))) return "—";
  const date = new Date(Number(value));
  return Number.isNaN(date.getTime()) ? "—" : date.toLocaleTimeString("zh-CN", { hour12: false });
}

function hookActivityResourceText(value) {
  if (!value || typeof value !== "object") return value == null ? "" : String(value);
  if (Array.isArray(value)) return value.map(hookActivityResourceText).filter(Boolean).join(", ");
  return Object.entries(value)
    .filter(([, item]) => item != null && item !== "")
    .map(([key, item]) => `${key}=${typeof item === "object" ? hookActivityResourceText(item) : item}`)
    .join(" · ");
}

function SyscallHooks({ modules }) {
  const [enabled, setEnabled] = useState(true);
  const [scopeMode, setScopeMode] = useState("all");
  const [selectedModule, setSelectedModule] = useState("");
  const [rvaBegin, setRvaBegin] = useState("0x0");
  const [rvaEnd, setRvaEnd] = useState("0x1000");
  const [filterMode, setFilterMode] = useState("all");
  const [numberText, setNumberText] = useState("");
  const [events, setEvents] = useState([]);
  const [stats, setStats] = useState(null);
  const [phase, setPhase] = useState("all");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [savedLabel, setSavedLabel] = useState("");
  const [appliedKey, setAppliedKey] = useState("");

  const refreshEvents = useCallback(async () => {
    const result = await api.syscallMonitor(256);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setEvents(Array.isArray(result.value?.events) ? result.value.events : []);
    setStats(result.value || null);
  }, []);

  useEffect(() => {
    let live = true;
    let timer = 0;
    const load = async () => {
      const config = await api.syscallConfigGet();
      if (!live) return;
      if (config.ok) {
        const numbers = Array.isArray(config.value?.numbers) ? config.value.numbers : [];
        const nextEnabled = config.value?.enabled !== false;
        const nextFilterMode = config.value?.mode === "selected" ? "selected" : "all";
        const nextScopeMode = ["module", "rva"].includes(config.value?.scope) ? config.value.scope : "all";
        const nextModule = String(config.value?.module || "");
        const nextRvaBegin = String(config.value?.rva_begin || "0x0");
        const nextRvaEnd = String(config.value?.rva_end || "0x1000");
        const parsedNumbers = numbers.map(parseOneSyscallNumber).filter((number) => number != null);
        setEnabled(nextEnabled);
        setFilterMode(nextFilterMode);
        setNumberText(numbers.join(", "));
        setScopeMode(nextScopeMode);
        setSelectedModule(nextModule);
        setRvaBegin(nextRvaBegin);
        setRvaEnd(nextRvaEnd);
        setAppliedKey(syscallConfigKey(nextEnabled, nextScopeMode, nextModule, nextRvaBegin, nextRvaEnd, nextFilterMode, parsedNumbers));
        setSavedLabel(syscallConfigLabel(nextEnabled, nextScopeMode, nextModule, nextRvaBegin, nextRvaEnd, parsedNumbers));
      } else {
        setError(config.error);
      }
      await refreshEvents();
      const poll = async () => {
        await refreshEvents();
        if (live) timer = window.setTimeout(poll, 1500);
      };
      if (live) timer = window.setTimeout(poll, 1500);
    };
    load();
    return () => { live = false; window.clearTimeout(timer); };
  }, [refreshEvents]);

  useEffect(() => {
    if (!selectedModule && modules.length) {
      setSelectedModule((modules.find((module) => module.main) || modules[0]).name);
    }
  }, [modules, selectedModule]);

  const observedNumbers = useMemo(() => {
    const values = new Map();
    events.forEach((event) => {
      const parsed = parseOneSyscallNumber(event.number);
      if (parsed != null) values.set(parsed, canonicalSyscallNumber(parsed));
    });
    return [...values.entries()].sort((left, right) => left[0] - right[0]).slice(0, 80);
  }, [events]);
  const numberTokens = useMemo(() => splitValueTokens(numberText), [numberText]);
  const parsedDraft = useMemo(() => parseSyscallNumbers(numberText), [numberText]);
  const selectedSet = useMemo(() => new Set(parsedDraft.numbers), [parsedDraft.numbers]);
  const draftKey = useMemo(
    () => syscallConfigKey(enabled, scopeMode, selectedModule, rvaBegin, rvaEnd, filterMode, parsedDraft.numbers),
    [enabled, scopeMode, selectedModule, rvaBegin, rvaEnd, filterMode, parsedDraft.numbers],
  );
  const dirty = Boolean(appliedKey) && draftKey !== appliedKey;
  const visibleEvents = [...events]
    .filter((event) => phase === "all" || event.phase === phase)
    .reverse();
  const moduleInfo = modules.find((module) => module.name === selectedModule) || null;

  function toggleNumber(number) {
    const next = new Set(parsedDraft.numbers);
    if (next.has(number)) next.delete(number);
    else next.add(number);
    setFilterMode("selected");
    setNumberText([...next].sort((left, right) => left - right).map(canonicalSyscallNumber).join(", "));
  }

  async function apply() {
    if (busy) return;
    if (parsedDraft.error) {
      setError(parsedDraft.error);
      return;
    }
    const numbers = filterMode === "all" ? [] : parsedDraft.numbers;
    if (filterMode === "selected" && numbers.length === 0) {
      setError("“仅指定编号”至少需要一个 syscall 号");
      return;
    }
    if (scopeMode !== "all" && !selectedModule) {
      setError("请选择一个已加载模块");
      return;
    }
    if (scopeMode === "rva") {
      const begin = parseUnsignedInteger(rvaBegin);
      const end = parseUnsignedInteger(rvaEnd);
      if (begin == null || end == null || end <= begin) {
        setError("RVA End 必须大于有效的 RVA Begin");
        return;
      }
    }
    setBusy(true);
    const result = await api.syscallConfigSet(enabled, numbers.map(String), scopeMode, selectedModule, rvaBegin, rvaEnd);
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    const saved = Array.isArray(result.value?.numbers) ? result.value.numbers : [];
    setNumberText(saved.join(", "));
    setFilterMode(result.value?.mode === "selected" ? "selected" : "all");
    setError("");
    const savedNumbers = saved.map(parseOneSyscallNumber).filter((number) => number != null);
    const savedEnabled = result.value?.enabled !== false;
    const savedScope = ["module", "rva"].includes(result.value?.scope) ? result.value.scope : "all";
    const savedModule = String(result.value?.module || selectedModule || "");
    const savedRvaBegin = String(result.value?.rva_begin || rvaBegin);
    const savedRvaEnd = String(result.value?.rva_end || rvaEnd);
    const savedMode = result.value?.mode === "selected" ? "selected" : "all";
    setAppliedKey(syscallConfigKey(savedEnabled, savedScope, savedModule, savedRvaBegin, savedRvaEnd, savedMode, savedNumbers));
    setSavedLabel(syscallConfigLabel(savedEnabled, savedScope, savedModule, savedRvaBegin, savedRvaEnd, savedNumbers));
    await refreshEvents();
  }

  return <div className="pbh-syscall-view">
    <section className="pbh-syscall-config">
      <div className="pbh-syscall-title">
        <div><b>系统调用监控</b><span className={enabled ? "running" : "stopped"}><i />{enabled ? "已启用" : "已停用"}</span></div>
        <label className="pbh-syscall-toggle" title="启用或停用系统调用事件采集"><input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} /><span /><em>{enabled ? "开" : "关"}</em></label>
      </div>
      <div className="pbh-syscall-form">
        <section className="pbh-syscall-group">
          <div className="pbh-syscall-section-label"><b>调用来源</b></div>
          <div className="pbh-syscall-scope-mode">
            <button title="不限制调用地址" className={scopeMode === "all" ? "active" : ""} onClick={() => setScopeMode("all")}>全部地址</button>
            <button title="匹配整个已加载模块" className={scopeMode === "module" ? "active" : ""} onClick={() => setScopeMode("module")}>模块</button>
            <button title="匹配模块内的半开 RVA 范围" className={scopeMode === "rva" ? "active" : ""} onClick={() => setScopeMode("rva")}>RVA 范围</button>
          </div>
          {scopeMode !== "all" && <div className="pbh-syscall-module-scope">
            <label><span>模块</span><select value={selectedModule} onChange={(event) => setSelectedModule(event.target.value)}>{modules.map((module) => <option key={`${module.name}:${module.low}`} value={module.name}>{moduleLabel(module)}{module.main ? "（主模块）" : ""}</option>)}</select></label>
            <div><span>地址范围</span><code>{moduleInfo ? `${moduleInfo.low} — ${moduleInfo.high}` : "—"}</code></div>
          </div>}
          {scopeMode === "rva" && <div className="pbh-syscall-rva-editor"><RvaRangeEditor compact maxRanges={1} ranges={[{ begin: rvaBegin, end: rvaEnd }]} onChange={(next) => { setRvaBegin(next[0]?.begin || "0x0"); setRvaEnd(next[0]?.end || "0x1000"); }} /></div>}
        </section>
        <section className="pbh-syscall-group">
          <div className="pbh-syscall-section-label"><b>系统调用号</b></div>
          <div className="pbh-syscall-mode">
            <button className={filterMode === "all" ? "active" : ""} onClick={() => setFilterMode("all")}>全部</button>
            <button className={filterMode === "selected" ? "active" : ""} onClick={() => setFilterMode("selected")}>指定编号</button>
          </div>
          <div className="pbh-syscall-input"><span>编号</span><ValueTokenEditor disabled={filterMode === "all"} maxValues={128} values={numberTokens} onChange={(next) => setNumberText(next.join(", "))} normalize={normalizeSyscallToken} placeholder="输入编号后按 Enter" /></div>
          <div className="pbh-syscall-observed"><span>会话中出现</span><div>{observedNumbers.map(([number, label]) => <button key={number} className={selectedSet.has(number) && filterMode === "selected" ? "selected" : ""} onClick={() => toggleNumber(number)}>{label}</button>)}{observedNumbers.length === 0 && <em>—</em>}</div></div>
        </section>
      </div>
      {parsedDraft.error && <div className="pba-error">{parsedDraft.error}</div>}
      {error && error !== parsedDraft.error && <div className="pba-error">{error}</div>}
      <div className="pbh-syscall-apply"><span className={dirty ? "dirty" : ""}><i />{dirty ? "有未应用的更改" : savedLabel || "正在读取配置"}</span><button className="primary" disabled={busy || Boolean(parsedDraft.error) || (Boolean(appliedKey) && !dirty)} onClick={apply}>{busy ? "应用中…" : "应用"}</button></div>
    </section>
    <section className="pbh-syscall-log">
      <div className="pbh-syscall-log-head"><div><b>事件</b><span><i>{events.length}</i><i>窗口 {stats?.scan_limit || "2048"}</i><i className={Number(stats?.ring_dropped || 0) > 0 ? "warn" : ""}>丢弃 {stats?.ring_dropped || "0"}</i></span></div><div>{[["all", "全部"], ["entry", "入口"], ["exit", "出口"]].map(([value, label]) => <button key={value} className={phase === value ? "active" : ""} onClick={() => setPhase(value)}>{label}</button>)}</div></div>
      <div className="pbh-syscall-columns"><span>事件</span><span>编号</span><span>阶段</span><span>参数 / 返回值</span></div>
      <div className="pba-list pbh-syscall-rows">
        {visibleEvents.length === 0 && <div className="pba-empty"><b>暂无事件</b></div>}
        {visibleEvents.map((event) => <div className={`pbh-syscall-row ${event.phase}`} key={event.sequence}>
          <span><b>#{event.sequence}</b><small>TID {event.thread_id} · gen {event.generation}</small></span>
          <code>{event.number}<small>{event.number_decimal}</small></code>
          <em>{event.phase === "entry" ? "入口" : "出口"}</em>
          {event.phase === "entry"
            ? <div className="pbh-syscall-args">{(event.arguments || []).map((value, index) => <code key={index}><i>a{index}</i>{value}<small>{rawDecimal(value)}</small></code>)}</div>
            : <div className="pbh-syscall-result"><code><i>return</i>{event.return_value || "—"}<small>{rawDecimal(event.return_value)}</small></code><code><i>errno</i>{event.errno || "—"}<small>{rawDecimal(event.errno)}</small></code></div>}
        </div>)}
      </div>
    </section>
  </div>;
}

function syscallConfigKey(enabled, scope, module, rvaBegin, rvaEnd, mode, numbers) {
  const selectedNumbers = mode === "selected"
    ? [...numbers].sort((left, right) => left - right).join(",")
    : "";
  return JSON.stringify([
    Boolean(enabled),
    scope,
    scope === "all" ? "" : String(module || "").trim().toLowerCase(),
    scope === "rva" ? String(rvaBegin || "").trim().toLowerCase() : "",
    scope === "rva" ? String(rvaEnd || "").trim().toLowerCase() : "",
    mode,
    selectedNumbers,
  ]);
}

function splitValueTokens(value) {
  return String(value || "").split(/[\s,;]+/).map((item) => item.trim()).filter(Boolean);
}

function normalizeSyscallToken(value) {
  const text = String(value || "").trim();
  const parsed = parseOneSyscallNumber(text);
  return parsed == null ? text : canonicalSyscallNumber(parsed);
}

function syscallConfigLabel(enabled, scope, module, rvaBegin, rvaEnd, numbers) {
  const source = scope === "module"
    ? moduleBaseName(module) || "模块"
    : scope === "rva"
      ? `${moduleBaseName(module) || "模块"} ${rvaBegin}–${rvaEnd}`
      : "全部地址";
  return `${enabled ? "已启用" : "已停用"} · ${source} · ${numbers.length ? `${numbers.length} 个编号` : "全部编号"}`;
}

function parseSyscallNumbers(value) {
  const tokens = String(value || "").trim().split(/[\s,;]+/).filter(Boolean);
  const numbers = [];
  const seen = new Set();
  for (const token of tokens) {
    const parsed = parseOneSyscallNumber(token);
    if (parsed == null) return { numbers: [], error: `无效 syscall 号：${token}` };
    if (parsed > 0xfff) return { numbers: [], error: `syscall 号超出 0x000–0xFFF：${token}` };
    if (!seen.has(parsed)) {
      seen.add(parsed);
      numbers.push(parsed);
    }
  }
  numbers.sort((left, right) => left - right);
  return { numbers, error: "" };
}

function parseOneSyscallNumber(value) {
  const token = String(value || "").trim();
  if (!/^(?:0[xX][0-9a-fA-F]+|[0-9]+)$/.test(token)) return null;
  const parsed = Number.parseInt(token, token.toLowerCase().startsWith("0x") ? 16 : 10);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : null;
}

function parseUnsignedInteger(value) {
  const token = String(value || "").trim();
  if (!/^(?:0[xX][0-9a-fA-F]+|[0-9]+)$/.test(token)) return null;
  try {
    const parsed = BigInt(token);
    return parsed >= 0n && parsed <= 0xffffffffffffffffn ? parsed : null;
  } catch {
    return null;
  }
}

function canonicalSyscallNumber(value) {
  return `0x${Number(value).toString(16).padStart(3, "0")}`;
}

function HookDetail({ hook, events, output, busy, onBack, onGoto, onMutate, onRefresh, onError, pointerWidth }) {
  const address = normalizeAddress(hook.address) || hook.address;
  const callbacks = Array.isArray(hook.callbacks) ? hook.callbacks : [];
  const [selectedId, setSelectedId] = useState(callbacks[0]?.id || "");
  const [editorOpen, setEditorOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [scriptName, setScriptName] = useState("");
  const [source, setSource] = useState("");
  const [sourceMeta, setSourceMeta] = useState(null);
  const [sourceError, setSourceError] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [signatureOpen, setSignatureOpen] = useState(false);
  const active = callbacks.find((item) => String(item.id) === String(selectedId)) || callbacks[0] || null;
  const recent = [...events].reverse().filter((event) => normalizeAddress(event.address) === address).slice(0, 40);

  useEffect(() => {
    if (!active || creating) {
      setLoading(false);
      return;
    }
    let live = true;
    setLoading(true);
    setSourceError("");
    api.scriptGet(active.plugin).then((result) => {
      if (!live) return;
      setLoading(false);
      if (!result.ok) {
        setSource("");
        setSourceMeta(null);
        setSourceError(result.error);
        return;
      }
      setScriptName(active.plugin);
      setSource(String(result.value?.source || ""));
      setSourceMeta(result.value || null);
      setSourceError("");
    });
    return () => { live = false; };
  }, [active?.plugin, creating]);

  function beginCreate(selector = "hook.entry") {
    setCreating(true);
    setSelectedId("");
    setScriptName(`ui_hook_${address.slice(2)}_${Date.now().toString(36)}.py`);
    setSource(hookTemplate(address, selector, hook.display || symbolText(hook)));
    setSourceMeta(null);
    setSourceError("");
    setEditorOpen(true);
  }

  async function saveSource(draft) {
    if (saving) return false;
    const name = String(draft?.name || "").trim();
    const nextSource = String(draft?.source || "");
    if (!name || !nextSource.trim()) return false;
    setSaving(true);
    const result = creating
      ? await api.scriptInject(name, nextSource, "callback")
      : await api.scriptReplace(name, nextSource, sourceMeta?.kind || "callback");
    setSaving(false);
    if (!result.ok) {
      setSourceError(result.error);
      onError(result.error);
      return false;
    }
    setCreating(false);
    setEditorOpen(false);
    setScriptName(name);
    setSource(nextSource);
    setSourceMeta(result.value || sourceMeta);
    await onRefresh();
    return true;
  }

  async function unloadCallback() {
    if (!active || !window.confirm(`卸载 ${active.plugin} 及其 Hook 回调？`)) return;
    const result = await api.scriptRemove(active.plugin);
    if (!result.ok) {
      onError(result.error);
      return;
    }
    setSelectedId("");
    await onRefresh();
  }

  const pluginOutput = output.filter((line) => line.plugin === active?.plugin).slice(-80);
  return (
    <div className="pba-detail-scroll pbh-detail">
      <section className="pba-detail-section">
        <div className="pba-detail-title"><b>{hook.function_log ? "函数调用 Hook 详情" : "指令 Hook 详情"}</b><button onClick={onBack}>返回列表</button></div>
        <div className="pba-detail-grid">
          <Detail label="地址" value={address} mono />
          <Detail label="符号" value={hook.display || symbolText(hook)} mono />
          <Detail label="模块" value={hook.module || "未解析"} />
          <Detail label="来源" value={ownerText(hook)} />
          <Detail label="最近命中" value={recent[0] ? `#${recent[0].sequence} · TID ${recent[0].thread_id}` : "未命中"} />
          <Detail label="同步回调" value={`${callbacks.length} 个`} />
          {hook.function_log && <Detail label="签名状态" value={hook.signature_status === "resolved" ? `${hook.signature?.source || "未知来源"} · ${hook.signature?.confidence || "0"}%` : "缺失：仅原始 ABI"} />}
        </div>
        <div className="pbh-detail-actions">
          <button onClick={() => onGoto(address)}>在反汇编中定位</button>
          {hook.function_log && <button onClick={() => setSignatureOpen(true)}>{hook.signature_status === "resolved" ? "编辑函数签名" : "设置函数签名"}</button>}
          <button className="primary" onClick={() => beginCreate("hook.entry")}>＋ 入口接管</button>
          {!hook.function_log && <button onClick={() => beginCreate("hook.return")}>＋ 返回指令接管</button>}
          <button className="danger" disabled={busy || callbacks.length > 0} title={callbacks.length ? "先卸载回调脚本" : "移除原生 Hook"} onClick={() => onMutate(() => api.hookRemove(address)).then((value) => value && onBack())}>移除 Hook</button>
        </div>
      </section>

      {hook.function_log && <SignatureSummary signature={hook.signature} />}

      <section className="pba-detail-section">
        <div className="pba-detail-title"><b>同步接管</b><span>目标线程 · 执行前</span></div>
        {callbacks.length === 0 ? <div className="pba-detail-empty">仅记录 · 无接管</div> : (
          <>
            <div className="pba-binding-tabs">
              {callbacks.map((callback) => <button key={callback.id} className={String(active?.id) === String(callback.id) ? "active" : ""} onClick={() => setSelectedId(callback.id)}>{actorLabel(callback.created_by || callback.owner)} · {callback.selector} · {callback.callback}</button>)}
            </div>
            {active && <div className="pba-detail-grid compact">
               <Detail label="脚本" value={active.plugin} mono />
               <Detail label="创建者" value={actorLabel(active.created_by || active.owner)} />
               <Detail label="最后修改" value={actorLabel(active.modified_by || active.created_by || active.owner)} />
               <Detail label="事件" value={active.selector} mono />
              <Detail label="函数" value={active.callback} mono />
              <Detail label="分发筛选器" value={`${active.selector} @ ${address}${active.thread_id == null ? " · 全部线程" : ` · TID ${active.thread_id}`}`} mono />
              <Detail label="回调说明" value={active.description || "旧回调未提供说明"} />
              <Detail label="模式" value={active.once ? "一次性" : "持续"} />
              <Detail label="最近代次" value={active.last_generation || "0"} />
            </div>}
            {active?.last_return != null && <HookReturn value={active.last_return} error={active.last_error} />}
            {active?.last_error && <div className="pba-error">{active.last_error}</div>}
            <div className="pbe-code-actions">
              <div><b>{scriptName || active?.plugin}</b><span>{loading ? "读取源码…" : source ? `${source.split(/\r?\n/).length} 行 Python · generation ${sourceMeta?.generation || "—"}` : "源码不可用"}</span></div>
              <button disabled={loading || !active?.source_available} onClick={() => setEditorOpen(true)}>查看 / 编辑代码</button>
              <button className="danger" onClick={unloadCallback}>卸载脚本</button>
            </div>
            {sourceError && <div className="pba-error">{sourceError}</div>}
            <div className="pbe-output"><div><b>回调输出</b><span>{pluginOutput.length} 行</span></div><pre>{pluginOutput.length ? pluginOutput.map((line) => `[${line.seq}] ${line.line}`).join("\n") : "暂无输出"}</pre></div>
          </>
        )}
      </section>

      <section className="pba-detail-section">
        <div className="pba-detail-title"><b>{hook.function_log ? "函数调用日志" : "最近命中"}</b><span>{hook.function_log ? "签名参数 · 返回值" : "寄存器 · 栈参数"}</span></div>
        <div className="pbh-hit-list">
          {recent.length === 0 && <div className="pba-detail-empty">无命中</div>}
          {recent.map((event) => <HitRow key={event.sequence} event={event} pointerWidth={pointerWidth} onGoto={onGoto} />)}
        </div>
      </section>
      <CallbackEditorDialog
        open={editorOpen}
        creating={creating}
        name={scriptName}
        source={source}
        meta={sourceMeta}
        error={sourceError}
        loading={loading}
        saving={saving}
        readOnly={!creating && !active?.source_available}
        callbackKind="Hook 接管"
        onClose={() => { setEditorOpen(false); if (creating) setCreating(false); }}
        onApply={saveSource}
      />
      <HookSignatureDialog
        open={signatureOpen}
        title={hook.display || symbolText(hook)}
        existing={hook.signature}
        onClose={() => setSignatureOpen(false)}
        onSave={async (draft) => {
          const result = await onMutate(() => api.hookSignatureSet(address, draft.signature, draft.source, draft.confidence));
          if (!result) return false;
          setSignatureOpen(false);
          await onRefresh();
          return true;
        }}
        onRemove={hook.signature_status === "resolved" ? async () => {
          const result = await onMutate(() => api.hookSignatureRemove(address));
          if (!result) return false;
          setSignatureOpen(false);
          await onRefresh();
          return true;
        } : null}
      />
    </div>
  );
}

function DllHooks({ modules, functionAddresses, busy, onGoto, onSelectHook, onMutate, onError }) {
  const [filter, setFilter] = useState("");
  const [selectedName, setSelectedName] = useState("");
  const [exports, setExports] = useState([]);
  const [exportFilter, setExportFilter] = useState("");
  const [exportPage, setExportPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [signatureTarget, setSignatureTarget] = useState(null);
  const functionLogged = useMemo(() => new Set(functionAddresses), [functionAddresses]);
  const hookCountByModule = useMemo(
    () => countHooksByModule(functionAddresses, modules),
    [functionAddresses, modules],
  );
  const selected = modules.find((module) => module.name === selectedName) || null;
  const visibleModules = modules.filter((module) => moduleLabel(module).toLowerCase().includes(filter.toLowerCase()));

  async function choose(module) {
    setSelectedName(module.name);
    setExportPage(0);
    setLoading(true);
    const result = await api.moduleExports(module.name);
    setLoading(false);
    if (!result.ok) {
      setExports([]);
      onError(result.error);
      return;
    }
    setExports(Array.isArray(result.value?.exports) ? result.value.exports : []);
  }

  async function hookAll() {
    if (!selected) return;
    const result = await onMutate(() => api.hookModule(selected.name));
    if (result) await choose(selected);
  }

  if (selected) {
    const visible = exports.filter((entry) => String(entry.name || "").toLowerCase().includes(exportFilter.toLowerCase()));
    const exportPageCount = Math.max(1, Math.ceil(visible.length / EXPORT_PAGE_SIZE));
    const safeExportPage = Math.min(exportPage, exportPageCount - 1);
    const pageExports = visible.slice(safeExportPage * EXPORT_PAGE_SIZE, (safeExportPage + 1) * EXPORT_PAGE_SIZE);
    const unique = new Set(exports.map((entry) => normalizeAddress(entry.address))).size;
    const armed = new Set(exports.map((entry) => normalizeAddress(entry.address)).filter((address) => functionLogged.has(address))).size;
    return (
      <div className="pbh-body pbh-dll-detail">
        <div className="pbh-dll-summary">
          <button onClick={() => { setSelectedName(""); setExports([]); }}>← DLL 列表</button>
          <div><b>{moduleLabel(selected)}</b><span>{exports.length} 个导出 · {unique} 个唯一地址 · 已记录调用 {armed}</span></div>
          <button className="primary" disabled={busy || loading || exports.length === 0} onClick={hookAll}>一键记录全部调用</button>
        </div>
          <div className="pbh-signature-notice"><b>函数签名未包含在导出表中</b><span>未登记签名时记录原始 ABI</span></div>
        <div className="pbh-export-search"><input value={exportFilter} onChange={(event) => { setExportFilter(event.target.value); setExportPage(0); }} placeholder="筛选导出函数" /></div>
        <div className="pbh-export-head"><span>导出函数</span><span>地址</span><span>状态</span><span>操作</span></div>
        <div className="pba-list pbh-export-list">
          {loading && <div className="pba-empty"><b>读取导出表…</b></div>}
          {!loading && pageExports.map((entry, index) => {
            const address = normalizeAddress(entry.address);
            const isFunctionLogged = functionLogged.has(address);
            return <div className="pbh-export-row" key={`${entry.name}:${address}:${index}`}>
              <b>{entry.name || "<ordinal>"}</b><code>{address}</code><em className={isFunctionLogged ? "armed" : ""}>{isFunctionLogged ? "API 调用已记录" : "未记录 API Hook"}</em>
              <span>
                <button onClick={() => onGoto(address)}>定位</button>
                {isFunctionLogged && <button onClick={() => onSelectHook(address)}>详情</button>}
                {!isFunctionLogged && <button disabled={busy} onClick={() => setSignatureTarget({ address, name: entry.name || address })}>签名并记录</button>}
              </span>
            </div>;
          })}
        </div>
        <HookPager
          page={safeExportPage}
          pageCount={exportPageCount}
          total={visible.length}
          pageSize={EXPORT_PAGE_SIZE}
          onPage={setExportPage}
        />
        <HookSignatureDialog
          open={Boolean(signatureTarget)}
          title={signatureTarget?.name || "函数"}
          existing={null}
          onClose={() => setSignatureTarget(null)}
          onSave={async (draft) => {
            const result = await onMutate(() => api.hookFunctionSet(signatureTarget.address, draft.signature, draft.source, draft.confidence));
            if (!result) return false;
            setSignatureTarget(null);
            return true;
          }}
        />
      </div>
    );
  }

  return (
    <div className="pbh-body">
      <div className="pbh-module-search"><input value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="筛选已加载 DLL" /><span>{modules.length} 个模块</span></div>
      <div className="pbh-module-head"><span>DLL</span><span>地址范围</span><span>调用记录</span></div>
      <div className="pba-list pbh-module-list">
        {visibleModules.map((module) => {
          const key = moduleRangeKey(module);
          const count = hookCountByModule.get(key) || 0;
          return <button key={`${module.name}:${module.low}`} className="pbh-module-row" onClick={() => choose(module)}>
            <span><b>{moduleLabel(module)}</b><small>{module.main ? "主模块" : module.name}</small></span>
            <code>{module.low} — {module.high}</code>
            <em className={count ? "armed" : ""}>{count || "—"}</em>
          </button>;
        })}
      </div>
    </div>
  );
}

function HookPager({ page, pageCount, total, pageSize, onPage }) {
  const start = total ? page * pageSize + 1 : 0;
  const end = Math.min(total, (page + 1) * pageSize);
  return <div className="pbh-pager">
    <span>显示 {start}–{end} / {total}</span>
    <div>
      <button disabled={page <= 0} onClick={() => onPage(0)}>首页</button>
      <button disabled={page <= 0} onClick={() => onPage(page - 1)}>上一页</button>
      <b>{page + 1} / {pageCount}</b>
      <button disabled={page + 1 >= pageCount} onClick={() => onPage(page + 1)}>下一页</button>
      <button disabled={page + 1 >= pageCount} onClick={() => onPage(pageCount - 1)}>末页</button>
    </div>
  </div>;
}

function SignatureSummary({ signature }) {
  if (!signature) {
    return <section className="pba-detail-section pbh-signature-missing"><div className="pba-detail-title"><b>函数签名</b><span>未登记</span></div><div className="pba-detail-grid compact"><Detail label="采集格式" value="原始 ABI" /><Detail label="类型信息" value="不可用" /></div></section>;
  }
  const parameters = Array.isArray(signature.parameters) ? signature.parameters : [];
  return <section className="pba-detail-section pbh-signature-summary">
    <div className="pba-detail-title"><b>函数签名</b><span>{signature.source} · 置信度 {signature.confidence}% · {signature.calling_convention}</span></div>
    <code className="pbh-prototype">{signature.prototype}</code>
    <div className="pbh-signature-head"><span>参数</span><span>类型</span><span>大小</span><span>采集位置</span></div>
    <div className="pbh-signature-rows">
      {parameters.map((parameter) => <div key={`${parameter.index}:${parameter.name}`}><b>{parameter.name}</b><code>{parameter.type}</code><span>{parameter.size ? `${parameter.size} B` : "未知"}</span><em>{Number(parameter.index) < 4 ? `ABI #${parameter.index}` : `栈 #${Number(parameter.index) - 4}`}</em></div>)}
      {parameters.length === 0 && <div className="pba-detail-empty">无参数</div>}
    </div>
    <div className="pbh-return-type"><span>返回</span><code>{signature.return_type?.type || "未知"}</code><b>{signature.return_type?.size != null ? `${signature.return_type.size} B` : "大小未知"}</b></div>
  </section>;
}

function HookSignatureDialog({ open, title, existing, onClose, onSave, onRemove = null }) {
  const [signature, setSignature] = useState("");
  const [source, setSource] = useState("manual");
  const [confidence, setConfidence] = useState("100");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) return;
    setSignature(existing?.prototype || "");
    setSource(existing?.source || "manual");
    setConfidence(String(existing?.confidence ?? "100"));
  }, [open, existing?.prototype, existing?.source, existing?.confidence]);

  if (!open) return null;
  async function save() {
    if (saving || !signature.trim()) return;
    setSaving(true);
    await onSave?.({ signature: signature.trim(), source, confidence });
    setSaving(false);
  }
  return createPortal(<div className="pba-editor-backdrop pbh-signature-backdrop" role="presentation">
    <section className="pbh-signature-dialog" role="dialog" aria-modal="true" aria-label="函数签名">
      <header><div><b>函数签名与 ABI 解析</b><span>{title}</span></div><button onClick={onClose}>×</button></header>
      <div className="pbh-signature-form">
        <label><span>C/C++ 原型</span><textarea value={signature} onChange={(event) => setSignature(event.target.value)} spellCheck="false" placeholder="例如：int DemoApi(int value);" autoFocus /></label>
        <div>
          <label><span>签名来源</span><select value={source} onChange={(event) => setSource(event.target.value)}><option value="pdb">PDB</option><option value="header">头文件</option><option value="manual">人工声明</option><option value="ai_inferred">AI 反推</option></select></label>
          <label><span>置信度 0–100</span><input value={confidence} onChange={(event) => setConfidence(event.target.value.replace(/\D/g, "").slice(0, 3))} /></label>
        </div>
        <p className="pbh-signature-source-note">签名来源和置信度将随 Hook 保存。</p>
        <aside><b>解析范围</b><div><span>x64 · GPR / XMM / Stack</span><span>x86 · 调用约定</span><span>最多 16 个参数</span><span>未知类型 · 未解析</span></div></aside>
      </div>
      <footer>{onRemove && <button className="danger" disabled={saving} onClick={async () => { setSaving(true); await onRemove(); setSaving(false); }}>删除签名</button>}<span /><button disabled={saving} onClick={onClose}>取消</button><button className="primary" disabled={saving || !signature.trim() || !confidence} onClick={save}>{saving ? "正在应用…" : existing ? "更新签名" : "按签名开始记录"}</button></footer>
    </section>
  </div>, document.body);
}

function HitRow({ event, pointerWidth, onGoto }) {
  const [expanded, setExpanded] = useState(false);
  const status = hookCaptureLabel(event);
  return <div className={`pbh-hit-record ${expanded ? "expanded" : ""}`}>
    <button className={`pbh-hit-row ${event.signature_status === "resolved" ? "typed" : "raw"}`} aria-expanded={expanded} onClick={() => setExpanded((value) => !value)}>
      <span><b>#{event.sequence} · {formatHookTime(event)}</b><small>{event.kind === "return" ? "返回" : "入口"} · TID {event.thread_id} · {status}</small></span>
      <code title={event.kind === "return" ? "声明返回类型" : "声明参数类型、大小和值"}>{hookEventPayload(event)}</code>
      <em>{expanded ? "收起" : "展开参数"}</em>
    </button>
    {expanded && <HookEventContext event={event} pointerWidth={pointerWidth} onGoto={onGoto} compact />}
  </div>;
}

function HookEventContext({ event, pointerWidth, onGoto, compact = false }) {
  if (hookEventType(event) === "syscall") return <SyscallEventContext event={event} compact={compact} />;
  const width = Number(pointerWidth || 8) === 4 ? 4 : 8;
  const typedArguments = Array.isArray(event.typed_arguments) ? event.typed_arguments : [];
  const rawArguments = Array.isArray(event.arguments) ? event.arguments : [];
  const isReturn = event.kind === "return";
  const isInstruction = hookEventType(event) === "instruction";
  const rows = isReturn
    ? [returnContextRow(event, width)]
    : typedArguments.length
      ? typedArguments.map((argument, index) => typedContextRow(argument, index, event.signature, width))
      : rawArguments.map((value, index) => rawContextRow(value, index, width, isInstruction));
  const signature = event.signature;
  return <section className={`pbh-event-context ${compact ? "compact" : ""}`}>
    <div className="pbh-context-meta">
      <span><b>事件</b>#{event.sequence}</span>
      <span><b>时间</b>{formatHookTime(event, true)}</span>
      <span><b>线程</b>TID {event.thread_id}</span>
      <span><b>采集</b>{hookCaptureLabel(event)}</span>
      <span><b>地址</b><code>{event.address}</code></span>
      {(event.module || event.symbol) && <span><b>符号</b><code>{event.display || [event.module, event.symbol].filter(Boolean).join("!")}</code></span>}
    </div>
    {signature && <div className="pbh-context-signature"><span>{signature.source} · {signature.confidence}% · {signature.calling_convention}</span><code>{signature.prototype}</code></div>}
    {!signature && !isInstruction && <div className="pbh-context-warning">未登记签名 · 原始 ABI</div>}
    <div className="pbh-context-table">
      <div className="pbh-context-head"><span>槽位 / 参数</span><span>采集位置</span><span>声明类型</span><span>原始值</span><span>解析值 / 十进制</span></div>
      {rows.map((row) => <div className="pbh-context-row" key={row.key}>
        <b>{row.name}</b><em>{row.location}</em><span>{row.type}</span><code>{row.raw}</code><code>{row.display}</code>
      </div>)}
      {rows.length === 0 && <div className="pbh-context-empty">无参数槽位</div>}
    </div>
    <div className="pbh-context-actions">
      <details><summary>完整事件 JSON</summary><pre>{JSON.stringify(event, null, 2)}</pre></details>
      {onGoto && <button onClick={() => onGoto(event.address)}>在反汇编中定位</button>}
    </div>
  </section>;
}

function SyscallEventContext({ event, compact = false }) {
  const isReturn = event.kind === "return";
  const rows = isReturn
    ? [
        { key: "return", name: "返回值", location: "返回槽", raw: event.return_value ?? "—" },
        { key: "errno", name: "错误码", location: "errno", raw: event.errno ?? "—" },
      ]
    : (Array.isArray(event.arguments) ? event.arguments : []).map((value, index) => ({
        key: `argument:${index}`,
        name: `参数 a${index}`,
        location: `syscall arg #${index}`,
        raw: value,
      }));
  return <section className={`pbh-event-context ${compact ? "compact" : ""}`}>
    <div className="pbh-context-meta">
      <span><b>事件</b>Syscall #{event.sequence}</span>
      <span><b>时间</b>{formatHookTime(event, true)}</span>
      <span><b>线程</b>TID {event.thread_id}</span>
      <span><b>编号</b><code>{event.number}</code></span>
      <span><b>阶段</b>{isReturn ? "出口" : "入口"}</span>
      <span><b>代次</b>{event.generation || "0"}</span>
    </div>
    <div className="pbh-context-table">
      <div className="pbh-context-head"><span>字段</span><span>采集位置</span><span>类型</span><span>原始值</span><span>十进制</span></div>
      {rows.map((row) => <div className="pbh-context-row" key={row.key}>
        <b>{row.name}</b><em>{row.location}</em><span>无符号整数</span><code>{String(row.raw ?? "—")}</code><code>{rawDecimal(row.raw)}</code>
      </div>)}
      {rows.length === 0 && <div className="pbh-context-empty">无参数槽位</div>}
    </div>
    <div className="pbh-context-actions">
      <details><summary>完整事件 JSON</summary><pre>{JSON.stringify(event, null, 2)}</pre></details>
    </div>
  </section>;
}

function typedContextRow(argument, fallbackIndex, signature, pointerWidth) {
  const index = Number(argument?.index ?? fallbackIndex);
  const size = argument?.size ? `${argument.size} B` : "大小未知";
  return {
    key: `typed:${index}`,
    name: argument?.name || `参数 #${index}`,
    location: typedAbiLocation(index, argument?.kind, signature?.calling_convention, pointerWidth),
    type: `${argument?.type || "未知"} · ${size}`,
    raw: String(argument?.raw ?? "—"),
    display: String(argument?.display ?? "未捕获"),
  };
}

function rawContextRow(value, index, pointerWidth, instruction) {
  return {
    key: `raw:${index}`,
    name: `${instruction ? "现场槽" : "ABI 槽"} #${index}`,
    location: rawAbiLocation(index, pointerWidth),
    type: "未声明",
    raw: String(value ?? "—"),
    display: rawDecimal(value),
  };
}

function returnContextRow(event, pointerWidth) {
  const typed = event.typed_return;
  const raw = typed?.raw ?? event.return_value ?? event.arguments?.[0];
  const floating = String(typed?.kind || "").includes("float");
  return {
    key: "return",
    name: "返回值",
    location: floating ? "XMM0" : pointerWidth === 4 ? "EAX" : "RAX",
    type: typed ? `${typed.type || "未知"} · ${typed.size ? `${typed.size} B` : "大小未知"}` : "未声明",
    raw: String(raw ?? "—"),
    display: typed?.display != null ? String(typed.display) : rawDecimal(raw),
  };
}

function rawAbiLocation(index, pointerWidth) {
  if (pointerWidth === 8) {
    if (index < 4) return ["RCX", "RDX", "R8", "R9"][index];
    return `[RSP+0x${(0x28 + (index - 4) * 8).toString(16)}]`;
  }
  if (index < 4) return `寄存器槽 #${index}`;
  return `[ESP+0x${(4 + (index - 4) * 4).toString(16)}]`;
}

function typedAbiLocation(index, kind, callingConvention, pointerWidth) {
  if (pointerWidth === 8) {
    if (index < 4) return String(kind || "").includes("float") ? `XMM${index}` : ["RCX", "RDX", "R8", "R9"][index];
    return `[RSP+0x${(0x28 + (index - 4) * 8).toString(16)}]`;
  }
  if (callingConvention === "fastcall" && index < 2 && !String(kind || "").includes("float")) return ["ECX", "EDX"][index];
  const stackIndex = callingConvention === "fastcall" ? Math.max(0, index - 2) : index;
  return `[ESP+0x${(4 + stackIndex * 4).toString(16)}]`;
}

function rawDecimal(value) {
  try { return BigInt(String(value)).toString(10); }
  catch { return String(value ?? "—"); }
}

function hookCaptureLabel(event) {
  if (hookEventType(event) === "syscall") return "系统调用现场";
  if (event.capture_status === "signature") return "已按函数签名解析";
  if (event.capture_status === "pre_signature_raw_abi") return "签名设置前：原始 ABI";
  if (event.capture_status === "raw_abi") return "未登记签名：原始 ABI";
  if (event.capture_status === "instruction") return "指令现场";
  return event.signature_status === "resolved" ? "已有签名" : "原始现场";
}

function hookEventPayload(event) {
  const args = Array.isArray(event.arguments) ? event.arguments : [];
  if (hookEventType(event) === "syscall") {
    return event.kind === "return"
      ? `return ${event.return_value ?? "—"}  ·  errno ${event.errno ?? "—"}`
      : args.map((value, index) => `a${index} ${value}`).join("  ·  ");
  }
  if (event.kind === "return") {
    return event.typed_return
      ? `${event.typed_return.type}${event.typed_return.size ? `[${event.typed_return.size}B]` : "[?]"} = ${event.typed_return.display}`
      : `原始返回槽 ${event.return_value ?? args[0] ?? "—"}`;
  }
  const typedArguments = Array.isArray(event.typed_arguments) ? event.typed_arguments : [];
  if (event.kind === "hit" && args.length === 0) return "指令命中";
  return typedArguments.length
    ? typedArguments.map((argument) => `${argument.name}: ${argument.type}${argument.size ? `[${argument.size}B]` : "[?]"} = ${argument.display}`).join("  ·  ")
    : `原始 ABI：${args.map((value, index) => `#${index} ${value}`).join("  ")}`;
}

function hookEventType(event) {
  if (event.hook_type === "syscall") return "syscall";
  if (event.hook_type === "api") return "api";
  return "instruction";
}

function hookSequence(event) {
  try { return BigInt(String(event?.sequence || "0")); }
  catch { return 0n; }
}

function hookTimestamp(event) {
  try { return BigInt(String(event?.timestamp_unix_ns || "0")); }
  catch { return 0n; }
}

function compareHookEvents(left, right) {
  const leftTime = hookTimestamp(left);
  const rightTime = hookTimestamp(right);
  if (leftTime !== rightTime) return leftTime < rightTime ? -1 : 1;
  const leftSequence = hookSequence(left);
  const rightSequence = hookSequence(right);
  if (leftSequence !== rightSequence) return leftSequence < rightSequence ? -1 : 1;
  return hookEventType(left).localeCompare(hookEventType(right));
}

function hookSequenceOrder(left, right) {
  const leftSequence = hookSequence(left);
  const rightSequence = hookSequence(right);
  return leftSequence === rightSequence ? 0 : leftSequence < rightSequence ? -1 : 1;
}

function minimumHookSequence(events) {
  if (!events.length) return null;
  return events.reduce((minimum, event) => {
    const sequence = hookSequence(event);
    return minimum == null || sequence < minimum ? sequence : minimum;
  }, null);
}

function sequenceRange(events) {
  if (!events.length) return "—";
  return `${events[0].sequence}—${events[events.length - 1].sequence}`;
}

function pairHookEvents(events) {
  const rows = events.map((event) => ({ ...event }));
  const stacks = new Map();
  const eventKey = (event) => `${hookEventType(event)}:${event.sequence}`;
  const bySequence = new Map(rows.map((event) => [eventKey(event), event]));
  [...rows].sort(compareHookEvents).forEach((event) => {
    const type = hookEventType(event);
    if (type === "instruction") return;
    const target = type === "syscall" ? event.number : normalizeAddress(event.address) || event.address;
    const key = `${type}:${event.thread_id}:${target}`;
    const stack = stacks.get(key) || [];
    if (event.kind !== "return") {
      stack.push(eventKey(event));
      stacks.set(key, stack);
      return;
    }
    const entryKey = stack.pop();
    if (!entryKey) return;
    const entry = bySequence.get(entryKey);
    event.call_sequence = entry?.sequence;
    if (entry) entry.return_sequence = String(event.sequence);
  });
  return rows;
}

function formatHookTime(event, exact = false) {
  const nanoseconds = hookTimestamp(event);
  if (nanoseconds <= 0n) return exact ? `Agent 时间不可用 · 序列 #${event?.sequence || "0"}` : "时间不可用";
  const date = new Date(Number(nanoseconds / 1_000_000n));
  const pad = (value, width = 2) => String(value).padStart(width, "0");
  const rendered = `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`;
  return exact ? `${date.toISOString()} · ${nanoseconds} ns · 序列 #${event?.sequence || "0"}` : rendered;
}

function HookReturn({ value, error }) {
  const rendered = String(value ?? "None");
  let label = "检查后放行";
  if (error) label = "回调失败";
  else if (/['\"]action['\"]\s*:\s*['\"]return/.test(rendered)) label = "已跳过函数并返回";
  else if (/['\"]registers['\"]\s*:/.test(rendered)) label = "已改写寄存器";
  else if (/['\"]arguments['\"]\s*:/.test(rendered)) label = "已改写参数";
  return <div className={`pbe-result ${error ? "err" : "ok"}`}><div><b>最近返回：{label}</b></div><pre>{rendered}</pre></div>;
}

function Detail({ label, value, mono = false }) {
  return <div className="pba-detail-kv"><span>{label}</span><b className={mono ? "mono" : ""}>{value == null || value === "" ? "—" : value}</b></div>;
}

function countHits(events) {
  const counts = new Map();
  events.forEach((event) => {
    const address = normalizeAddress(event.address);
    if (address) counts.set(address, (counts.get(address) || 0) + 1);
  });
  return counts;
}

function hookTone(hook) {
  const actors = hookOwnerActors(hook);
  if (actors.includes("ai")) return "ai";
  if (actors.includes("human")) return "human";
  return isCallbackHook(hook) ? "strategy" : "target";
}

function hookOwnerActors(hook) {
  const actors = new Set();
  const add = (value) => {
    if (value === "operator") actors.add("human");
    else if (value === "human" || value === "ai") actors.add(value);
  };
  (Array.isArray(hook.plain_owners) ? hook.plain_owners : []).forEach(add);
  (Array.isArray(hook.callbacks) ? hook.callbacks : []).forEach((callback) => add(callback.created_by || callback.owner));
  return ["human", "ai"].filter((actor) => actors.has(actor));
}

function latestHookCallback(callbacks) {
  return [...callbacks].sort((left, right) => Number(right.last_generation || 0) - Number(left.last_generation || 0))[0] || null;
}

function isCallbackHook(hook) {
  const callbacks = Array.isArray(hook.callbacks) ? hook.callbacks : [];
  return callbacks.length > 0;
}

function scriptOriginLabel(script) {
  if (script.created_by === "ai") return "AI 创建";
  if (script.modified_by === "ai") return "AI 修改";
  if (script.created_by === "human" || script.created_by === "operator") return "人工创建";
  if (script.state === "agent_reported") return "MCP / Agent 已加载";
  return "外部脚本";
}

function isLoadedScript(script) {
  return script.state === "loaded" || script.state === "agent_reported" || String(script.registration?.state || "") === "1";
}

function hookType(hook) {
  const callbacks = Array.isArray(hook.callbacks) ? hook.callbacks : [];
  if (callbacks.some((item) => item.selector === "hook.return")) return "返回接管";
  if (callbacks.length) return "入口接管";
  if (hook.function_log) return "函数调用日志";
  return "指令捕获";
}

function symbolText(hook) {
  if (hook.module && hook.symbol) return `${moduleBaseName(hook.module)}!${hook.symbol}`;
  if (hook.module) return `${moduleBaseName(hook.module)}+${hook.offset || "0x0"}`;
  return "未解析符号";
}

function ownerText(hook) {
  const actors = hookOwnerActors(hook);
  return actors.map(actorLabel).join(" + ") || "外部 / 未知";
}

function actorLabel(actor) {
  if (actor === "ai") return "AI";
  if (actor === "human" || actor === "operator") return "人工";
  return actor || "外部 / 未知";
}

function moduleBaseName(value) {
  return String(value || "").split(/[\\/]/).pop() || "";
}

function moduleRangeKey(module) {
  return `${module?.name || ""}:${normalizeAddress(module?.low) || module?.low || ""}`;
}

function countHooksByModule(addresses, modules) {
  const ranges = modules
    .map((module) => {
      try {
        return {
          low: BigInt(module.low),
          high: BigInt(module.high),
          key: moduleRangeKey(module),
        };
      } catch {
        return null;
      }
    })
    .filter(Boolean)
    .sort((left, right) => left.low < right.low ? -1 : left.low > right.low ? 1 : 0);
  const values = addresses
    .map((address) => {
      try { return BigInt(address); } catch { return null; }
    })
    .filter((value) => value != null)
    .sort((left, right) => left < right ? -1 : left > right ? 1 : 0);
  const counts = new Map();
  let rangeIndex = 0;
  values.forEach((address) => {
    while (rangeIndex < ranges.length && address > ranges[rangeIndex].high) rangeIndex += 1;
    const range = ranges[rangeIndex];
    if (range && address >= range.low && address <= range.high) {
      counts.set(range.key, (counts.get(range.key) || 0) + 1);
    }
  });
  return counts;
}

function moduleLabel(module) {
  return moduleBaseName(module?.name) || "未命名模块";
}

function hookTemplate(address, selector, display) {
  const callback = selector === "hook.return" ? "on_return" : "on_entry";
  const response = selector === "hook.return"
    ? `    original = event["return_value"]\n    pb.print(f"HOOK_RETURN tid={event['tid']} address=0x{event['address']:x} value=0x{original:x}")\n\n    # 返回 None 保留原返回值；也可返回 {"return_value": 0x1234}\n    return None`
    : `    rows = pb.disasm(event["address"], 6)\n    pb.print(f"HOOK_ENTRY tid={event['tid']} address=0x{event['address']:x}")\n    for row in rows:\n        pb.print(f"  0x{row[0]:x}  {row[4]}")\n\n    # 返回 None 正常执行；可返回 {"arguments": [...] } 改参数，\n    # 或 {"action": "return", "return_value": 0x1234} 跳过函数。\n    return None`;
  return `import pb

# Hook 固定信息（供人工审计和 AI/MCP 读取）
HOOK_INFO = {
    "address": ${address},
    "symbol": ${JSON.stringify(display || "未解析符号")},
    "event": ${JSON.stringify(selector)},
    "purpose": "说明为什么接管此指令，以及允许修改什么",
}

def ${callback}(event):
    """检查 Hook 现场，并显式返回放行或接管决定。"""
${response}

def pb_init():
    pb.intercept(
        ${JSON.stringify(selector)},
        ${callback},
        address=HOOK_INFO["address"],
        description="说明为什么接管此指令，以及允许修改什么",
        once=False,
    )
`;
}
