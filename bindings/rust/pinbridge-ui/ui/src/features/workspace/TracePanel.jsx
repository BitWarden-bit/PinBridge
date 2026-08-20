import React, { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../../api";
import { normalizeAddress } from "../../address";
import { RvaRangeEditor, ValueTokenEditor } from "../../components/StructuredInputs";

const TRACE_KINDS = [
  { id: "exec", name: "执行", detail: "指令地址、长度和机器码" },
  { id: "memory", name: "内存", detail: "读写地址、宽度和值" },
  { id: "branch", name: "分支", detail: "跳转目标和是否跳转" },
  { id: "syscall", name: "系统调用", detail: "编号、阶段和返回值" },
  { id: "exception", name: "异常", detail: "上下文切换与异常信息" },
  { id: "registers", name: "寄存器", detail: "指令前寄存器增量快照" },
];

const INDEX_TYPES = [
  { id: "kind", name: "事件类型", label: "事件类型", placeholder: "exec" },
  { id: "address", name: "执行地址", label: "执行地址", placeholder: "0x140001000" },
  { id: "thread", name: "线程 ID", label: "线程 ID", placeholder: "1234" },
  { id: "sequence", name: "记录序号", label: "记录序号", placeholder: "100" },
  { id: "memory", name: "访问内存", label: "内存地址", placeholder: "0x200000" },
];

export default function TracePanel({ stopped, stopTick, onGoto }) {
  const [mode, setMode] = useState("record");
  const [modules, setModules] = useState([]);
  const [moduleName, setModuleName] = useState("");
  const [kinds, setKinds] = useState(["exec", "memory", "branch"]);
  const [rangeMode, setRangeMode] = useState("module");
  const [ranges, setRanges] = useState([{ begin: "0x0", end: "0x1000" }]);
  const [threadMode, setThreadMode] = useState("all");
  const [threads, setThreads] = useState([]);
  const [filename, setFilename] = useState("trace.pbtr");
  const [selection, setSelection] = useState(null);
  const [status, setStatus] = useState({ state: "idle", active: false, recorded: "0", dropped: "0" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");

  const [index, setIndex] = useState("kind");
  const [key, setKey] = useState("exec");
  const [limit, setLimit] = useState("50");
  const [before, setBefore] = useState("0");
  const [indexResult, setIndexResult] = useState(null);
  const [exportFormat, setExportFormat] = useState("json");
  const [exportPath, setExportPath] = useState("");

  const active = !!status?.active;
  const completed = status?.state === "complete" && !!status?.path;
  const selectedIndexType = INDEX_TYPES.find((item) => item.id === index) || INDEX_TYPES[0];
  const selectedModule = useMemo(
    () => modules.find((item) => item.name === moduleName) || null,
    [modules, moduleName],
  );

  const refreshModules = useCallback(async () => {
    const rows = await api.modules();
    if (!Array.isArray(rows)) return;
    setModules(rows);
    setModuleName((current) => {
      if (current && rows.some((item) => item.name === current)) return current;
      return rows.find((item) => item.main)?.name || rows[0]?.name || "";
    });
  }, []);

  const refreshStatus = useCallback(async (quiet = false) => {
    const result = await api.traceRecordStatus();
    if (!result.ok) {
      if (!quiet) setError(result.error);
      return null;
    }
    setStatus(result.value || {});
    return result.value;
  }, []);

  useEffect(() => {
    refreshModules();
    refreshStatus(true);
  }, [refreshModules, refreshStatus, stopTick]);

  useEffect(() => {
    if (!active) return undefined;
    const timer = window.setInterval(() => refreshStatus(true), 1000);
    return () => window.clearInterval(timer);
  }, [active, refreshStatus]);

  function invalidateSelection(update) {
    setSelection(null);
    setNotice("");
    update();
  }

  function toggleKind(kind) {
    invalidateSelection(() => {
      setKinds((current) => current.includes(kind)
        ? current.filter((item) => item !== kind)
        : [...current, kind]);
    });
  }

  function buildScope() {
    const parsedRanges = rangeMode === "ranges" ? parseRanges(ranges) : [];
    const parsedThreads = threadMode === "selected" ? parseThreads(threads) : [];
    return {
      module: moduleName,
      kinds,
      ...(parsedRanges.length ? { ranges: parsedRanges } : {}),
      ...(parsedThreads.length ? { threads: parsedThreads } : {}),
    };
  }

  async function queryScope() {
    if (busy || active) return;
    if (!moduleName) {
      setError("没有可用模块，请先连接目标进程。");
      return;
    }
    if (!kinds.length) {
      setError("至少选择一种要记录的内容。");
      return;
    }
    let query;
    try {
      query = buildScope();
    } catch (scopeError) {
      setError(String(scopeError.message || scopeError));
      return;
    }
    setBusy(true);
    setError("");
    setNotice("");
    const result = await api.traceScopeQuery(query);
    setBusy(false);
    if (!result.ok) {
      setSelection(null);
      setError(result.error);
      return;
    }
    setSelection(result.value);
    setNotice("范围已确认，可以开始录制。");
  }

  async function startRecording() {
    if (!selection || busy || active) return;
    setBusy(true);
    setError("");
    setNotice("");
    const result = await api.traceRecordStart({
      selection_id: String(selection.selection_id),
      expected_count: String(selection.selected_count),
      selection_digest: String(selection.selection_digest),
      filename: String(filename || "trace.pbtr"),
    });
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setStatus(result.value || {});
    setIndexResult(null);
    setExportPath("");
    setNotice("正在记录程序执行过程。");
  }

  async function stopRecording() {
    if (busy || !active) return;
    setBusy(true);
    setError("");
    setNotice("正在保存记录并生成本地查询库…");
    const result = await api.traceRecordStop();
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      setNotice("");
      return;
    }
    setStatus(result.value || {});
    setNotice("记录已保存到本地，可以查看或导出结果。");
    setMode("results");
  }

  function indexArgs(extra = {}) {
    const boundedLimit = Math.max(1, Math.min(256, Number.parseInt(limit, 10) || 50));
    return {
      index,
      key: String(key).trim(),
      limit: String(boundedLimit),
      ...(before && before !== "0" ? { before: String(before) } : {}),
      payload: true,
      metadata: false,
      ...extra,
    };
  }

  async function runIndex(nextBefore = null) {
    if (busy || !completed || !String(key).trim()) return;
    setBusy(true);
    setError("");
    setNotice("");
    const args = indexArgs(nextBefore == null ? {} : { before: String(nextBefore) });
    const result = await api.traceIndexQuery(args);
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setBefore(String(args.before || "0"));
    setIndexResult(result.value || null);
  }

  async function exportIndex() {
    if (busy || !completed || !String(key).trim()) return;
    setBusy(true);
    setError("");
    setNotice("");
    const result = await api.traceIndexExport(indexArgs({
      format: exportFormat,
      delivery: "file",
      filename: `trace-${index}-${safeName(key)}`,
    }));
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setExportPath(String(result.value?.path || ""));
    setNotice(`已导出 ${result.value?.rows || "0"} 条记录。`);
  }

  return (
    <section className="pba-panel pbt-panel">
      <header className="pba-panel-head">
        <div>
          <b>Trace</b>
          <span>记录程序执行过程 · {stopped ? "目标已暂停" : "目标运行中"}</span>
        </div>
        <div className="pbt-head-actions">
          <span className="pbt-head-stat"><i className={active ? "live" : ""} />{status?.recorded || "0"} 条</span>
          <span className={`pbt-head-stat ${String(status?.dropped || "0") !== "0" ? "warn" : ""}`}>丢弃 {status?.dropped || "0"}</span>
          <i className={`pbt-state ${active ? "active" : status?.state || "idle"}`}>{traceState(status?.state)}</i>
          <button disabled={busy} onClick={() => { refreshModules(); refreshStatus(); }}>刷新</button>
        </div>
      </header>

      <nav className="pbe-mode-tabs" aria-label="Trace 页面">
        <button className={mode === "record" ? "active" : ""} onClick={() => setMode("record")}>录制设置</button>
        <button className={mode === "results" ? "active" : ""} onClick={() => setMode("results")}>记录结果</button>
      </nav>

      {error && <div className="pba-error pbe-global-error" role="alert">{error}</div>}
      {notice && <div className="pbt-notice">{notice}</div>}

      {mode === "record" ? (
      <div className="pbt-subpage-scroll">
        <section className="pbt-status-card">
          <header className="pbt-tool-head">
            <div><b>录制状态</b><span>{active ? "正在接收程序执行记录" : completed ? "本次录制已保存" : "等待开始录制"}</span></div>
            <em className={active ? "recording" : status?.state || "idle"}><i />{traceState(status?.state)}</em>
          </header>
          <RecorderStats status={status} />
          <TraceArtifact status={status} />
        </section>

        <section className="pbt-setup-card">
          <header className="pbt-tool-head">
            <div><b>录制设置</b><span>选择记录哪些位置和内容</span></div>
            <em className={selection ? "ready" : "draft"}><i />{selection ? "范围已确认" : "待确认"}</em>
          </header>

          <div className="pbt-config-scroll">
            <div className="pbt-config-group">
              <div className="pbt-group-title"><b>记录范围</b><span>选择模块、位置和线程</span></div>
              <div className="pbt-grid">
                <label className="wide">
                  <span>模块</span>
                  <select disabled={active} value={moduleName} onChange={(event) => invalidateSelection(() => setModuleName(event.target.value))}>
                    {modules.map((item) => <option key={`${item.low}:${item.name}`} value={item.name}>{item.main ? "[主模块] " : ""}{item.name}</option>)}
                  </select>
                  {selectedModule && <small>{selectedModule.low} — {selectedModule.high}</small>}
                </label>
                <div className="pbt-scope-control wide">
                  <div className="pbt-control-label"><span>位置</span><small>{rangeMode === "module" ? "记录整个模块" : `${ranges.length} 个地址范围`}</small></div>
                  <div className="pbs-choice-tabs">
                    <button disabled={active} className={rangeMode === "module" ? "active" : ""} onClick={() => invalidateSelection(() => setRangeMode("module"))}>整个模块</button>
                    <button disabled={active} className={rangeMode === "ranges" ? "active" : ""} onClick={() => invalidateSelection(() => setRangeMode("ranges"))}>指定地址范围</button>
                  </div>
                  {rangeMode === "ranges" && <RvaRangeEditor disabled={active} ranges={ranges} onChange={(next) => invalidateSelection(() => setRanges(next))} />}
                </div>
                <div className="pbt-scope-control wide">
                  <div className="pbt-control-label"><span>线程</span><small>{threadMode === "all" ? "记录全部线程" : `${threads.length} 个线程`}</small></div>
                  <div className="pbs-choice-tabs">
                    <button disabled={active} className={threadMode === "all" ? "active" : ""} onClick={() => invalidateSelection(() => setThreadMode("all"))}>全部线程</button>
                    <button disabled={active} className={threadMode === "selected" ? "active" : ""} onClick={() => invalidateSelection(() => setThreadMode("selected"))}>指定线程</button>
                  </div>
                  {threadMode === "selected" && <ValueTokenEditor disabled={active} values={threads} onChange={(next) => invalidateSelection(() => setThreads(next))} placeholder="输入线程 ID 后按 Enter" normalize={normalizeThreadToken} />}
                </div>
                <label className="wide">
                  <span>保存文件名</span>
                  <input disabled={active} value={filename} placeholder="trace.pbtr" onChange={(event) => setFilename(event.target.value)} />
                </label>
              </div>
            </div>

            <div className="pbt-config-group">
              <div className="pbt-group-title"><b>记录内容</b><span>已选 {kinds.length} 项</span></div>
              <div className="pbt-kind-grid">
                {TRACE_KINDS.map((item) => (
                  <button key={item.id} disabled={active} className={kinds.includes(item.id) ? "active" : ""} onClick={() => toggleKind(item.id)}>
                    <i />
                    <span><b>{item.name}</b><small>{item.detail}</small></span>
                  </button>
                ))}
              </div>
            </div>

            {selection && (
              <div className="pbt-config-group pbt-confirmed-scope">
                <div className="pbt-group-title"><b>已确认的范围</b><span>{selection.selected_count} 个范围</span></div>
                <ScopeSummary value={selection} />
              </div>
            )}
          </div>

          <footer className="pbt-config-actions">
            <span className={selection ? "ready" : ""}><i />{active ? "录制期间设置不可修改" : selection ? "设置已确认，可以开始录制" : "确认设置后才能开始录制"}</span>
            <button disabled={busy || active} onClick={queryScope}>{busy && !active ? "正在确认…" : selection ? "重新确认" : "确认设置"}</button>
            {!active ? (
              <button className="primary" disabled={busy || !selection} onClick={startRecording}>开始录制</button>
            ) : (
              <button className="danger" disabled={busy} onClick={stopRecording}>{busy ? "正在整理…" : "停止录制"}</button>
            )}
          </footer>
        </section>
      </div>
      ) : (
      <div className="pbt-results-page">
        <section className="pbt-data-card">
          <header className="pbt-tool-head">
            <div><b>记录结果</b><span>{completed ? "从本地记录库按类型、地址或线程查找" : "录制完成后可在这里查看结果"}</span></div>
            <em className={completed ? "ready" : "idle"}><i />{completed ? "可以查看" : "暂无记录"}</em>
          </header>

          <div className="pbt-query-form">
            <div className="pbt-index-tabs" aria-label="查找方式">
              {INDEX_TYPES.map((item) => (
                <button disabled={!completed} key={item.id} className={index === item.id ? "active" : ""} onClick={() => { setIndex(item.id); setKey(item.placeholder); setBefore("0"); }}>
                  {item.name}
                </button>
              ))}
            </div>
            <div className="pbt-query-grid">
              <label className="wide">
                <span>{selectedIndexType.label}</span>
                {index === "kind" ? (
                  <select disabled={!completed} value={key} onChange={(event) => { setKey(event.target.value); setBefore("0"); }}>
                    {TRACE_KINDS.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
                  </select>
                ) : (
                  <input disabled={!completed} value={key} placeholder={selectedIndexType.placeholder} onChange={(event) => { setKey(event.target.value); setBefore("0"); }} />
                )}
              </label>
              <label><span>每页条数</span><input disabled={!completed} type="number" min="1" max="256" value={limit} onChange={(event) => setLimit(event.target.value)} /></label>
            </div>
            <div className="pbt-query-footer">
              <span className="pbt-query-hint">只查询本地记录，不访问目标程序</span>
              <select disabled={!completed} aria-label="导出格式" value={exportFormat} onChange={(event) => setExportFormat(event.target.value)}>
                <option value="json">JSON</option>
                <option value="csv">CSV</option>
                <option value="jsonl">JSONL（逐行）</option>
              </select>
              <button disabled={busy || !completed || !String(key).trim()} onClick={exportIndex}>导出结果</button>
              <button className="primary" disabled={busy || !completed || !String(key).trim()} onClick={() => runIndex()}>{busy ? "正在查找…" : "查找"}</button>
            </div>
          </div>
          {exportPath && <PathRow label="导出文件" path={exportPath} />}

          <section className="pbt-results">
            <header>
              <div><b>找到的记录</b><span>{indexResult ? `共匹配 ${indexResult.matched_total} 条，本页显示 ${indexResult.returned} 条` : completed ? "选择条件后点击查找" : "请先完成一次录制"}</span></div>
              {indexResult?.has_older && <button disabled={busy} onClick={() => runIndex(indexResult.next_before)}>查看更早记录</button>}
            </header>
            <div className="pbt-result-head"><span>序号</span><span>类型</span><span>线程</span><span>地址</span><span>详细信息</span></div>
            <div className="pbt-result-list">
              {!indexResult?.events?.length && <div className="pbt-table-empty"><i>⌕</i><b>{completed ? "还没有查找结果" : "还没有录制数据"}</b><span>{completed ? "选择上方条件后点击查找" : "完成一次录制后，结果会显示在这里"}</span></div>}
              {(indexResult?.events || []).map((event, row) => <TraceRow key={`${event.sequence || row}:${row}`} event={event} onGoto={onGoto} />)}
            </div>
          </section>
        </section>
      </div>
      )}
    </section>
  );
}

function RecorderStats({ status }) {
  const dropped = String(status?.dropped || "0");
  return (
    <div className="pbt-recorder-stats">
      <div><span>状态</span><b className={status?.active ? "ok" : ""}>{traceState(status?.state)}</b></div>
      <div><span>记录</span><b>{status?.recorded || "0"}</b></div>
      <div><span>丢弃</span><b className={dropped !== "0" ? "warn" : ""}>{dropped}</b></div>
      <div><span>文件大小</span><b>{formatBytes(status?.file_bytes)}</b></div>
    </div>
  );
}

function ScopeSummary({ value }) {
  return (
    <div className="pbt-scope-summary">
      <div className="pbt-scope-meta">
        <span><i>线程</i><b>{value.thread_scope === "all" ? "全部线程" : (value.threads || []).join(", ")}</b></span>
        <span><i>记录内容</i><b>{(value.kinds || []).map(traceKindName).join("、") || "未选择"}</b></span>
      </div>
      <div className="pbt-range-list">
        {(value.ranges || []).map((range, index) => (
          <div key={`${range.begin}:${range.end}`}>
            <span>范围 {index + 1}</span>
            <code>{range.rva_begin} — {range.rva_end}</code>
            <small>{range.begin} — {range.end}</small>
          </div>
        ))}
      </div>
    </div>
  );
}

function TraceArtifact({ status }) {
  if (!status?.path) return null;
  const database = String(status?.local_index?.database || "");
  return (
    <div className="pbt-session-block pbt-artifact">
      <div className="pbt-block-head"><b>记录文件</b><span>{formatBytes(status.file_bytes)}</span></div>
      <PathRow label="文件" path={status.path} />
      {database && <PathRow label="查询库" path={database} />}
    </div>
  );
}

function PathRow({ label, path }) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(path);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1000);
    } catch {
      setCopied(false);
    }
  }
  return <div className="pbt-path"><span>{label}</span><code title={path}>{path}</code><button onClick={copy}>{copied ? "已复制" : "复制路径"}</button></div>;
}

function TraceRow({ event, onGoto }) {
  const address = normalizeAddress(event.address);
  const payload = Object.fromEntries(Object.entries(event).filter(([field]) => !["sequence", "kind", "thread_id", "address"].includes(field)));
  const kind = String(event.kind || `kind-${event.kind_id || "unknown"}`);
  return (
    <div className="pbt-result-row">
      <code>#{event.sequence || "—"}</code>
      <span className={`pbt-kind-badge ${kind.replace(/[^a-z0-9_-]/gi, "")}`}><i />{traceKindName(event.kind) || `Kind ${event.kind_id || "?"}`}</span>
      <code className="pbt-thread">TID {event.thread_id || "—"}</code>
      {address ? <button onClick={() => onGoto?.(address)}>{address}</button> : <code>—</code>}
      <TracePayload value={payload} />
    </div>
  );
}

function TracePayload({ value }) {
  const entries = Object.entries(value);
  if (!entries.length) return <span className="pbt-payload-empty">—</span>;
  const title = JSON.stringify(value);
  return (
    <div className="pbt-payload" title={title}>
      {entries.slice(0, 4).map(([field, fieldValue]) => (
        <span key={field}><i>{field}</i><code>{formatTraceValue(fieldValue)}</code></span>
      ))}
      {entries.length > 4 && <em>+{entries.length - 4}</em>}
    </div>
  );
}

function parseRanges(rows) {
  if (!Array.isArray(rows) || !rows.length) throw new Error("至少添加一个地址范围。");
  if (rows.length > 16) throw new Error("地址范围最多 16 段。");
  return rows.map((range, index) => {
    const begin = parseUnsigned(range?.begin);
    const end = parseUnsigned(range?.end);
    if (begin == null || end == null) throw new Error(`第 ${index + 1} 个地址范围无效。`);
    if (end <= begin) throw new Error(`第 ${index + 1} 个范围的结束地址必须大于起始地址。`);
    return { rva_begin: String(range.begin).trim(), rva_end: String(range.end).trim() };
  });
}

function parseThreads(values) {
  if (!Array.isArray(values) || !values.length) throw new Error("至少添加一个线程 ID。");
  if (values.length > 64) throw new Error("最多指定 64 个线程。");
  return [...new Set(values.map((value) => {
    const text = normalizeThreadToken(value);
    if (!/^\d+$/.test(text) || Number(text) > 0xffffffff) throw new Error(`线程 ID ${value} 无效。`);
    return text;
  }))];
}

function normalizeThreadToken(value) {
  return String(value || "").trim().replace(/^tid\s*/i, "");
}

function parseUnsigned(value) {
  const text = String(value || "").trim();
  if (!/^(?:0x[0-9a-f]+|\d+)$/i.test(text)) return null;
  try {
    const parsed = BigInt(text);
    return parsed >= 0n ? parsed : null;
  } catch {
    return null;
  }
}

function safeName(value) {
  return String(value || "index").replace(/[^0-9a-z._-]+/gi, "_").slice(0, 64) || "index";
}

function traceKindName(kind) {
  return TRACE_KINDS.find((item) => item.id === kind)?.name || String(kind || "");
}

function formatTraceValue(value) {
  if (value == null) return "—";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

function formatBytes(value) {
  const bytes = Number(value || 0);
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

function traceState(state) {
  return ({ idle: "未开始", recording: "录制中", draining: "正在保存", complete: "已完成", failed: "失败" })[state] || state || "未知";
}
