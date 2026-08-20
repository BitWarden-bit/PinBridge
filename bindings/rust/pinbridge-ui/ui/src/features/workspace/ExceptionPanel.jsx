import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api";
import { normalizeAddress } from "../../address";
import CallbackEditorDialog from "./CallbackEditorDialog";

const TARGET_EXCEPTION_NAMES = {
  "0x80000003": "断点异常",
  "0x80000004": "单步异常",
  "0xc0000005": "访问冲突",
  "0xc000001d": "非法指令",
  "0xc000008c": "数组越界",
  "0xc000008d": "浮点非规格化操作数",
  "0xc000008e": "浮点除零",
  "0xc000008f": "浮点不精确结果",
  "0xc0000090": "浮点无效操作",
  "0xc0000091": "浮点溢出",
  "0xc0000092": "浮点栈检查",
  "0xc0000093": "浮点下溢",
  "0xc0000094": "整数除零",
  "0xc0000095": "整数溢出",
  "0xc00000fd": "栈溢出",
  "0xc0000374": "堆损坏",
  "0xe06d7363": "MSVC C++ 异常",
};

const PIN_EXCEPTION_CLASSES = {
  "0": "无分类",
  "1": "未知异常",
  "2": "访问错误",
  "3": "非法指令",
  "4": "整数错误",
  "5": "浮点错误",
  "6": "复合浮点错误",
  "7": "调试异常",
  "8": "操作系统异常",
};

const ACCESS_TYPES = { "0": "未知", "1": "读取", "2": "写入", "3": "执行" };

export default function ExceptionPanel({ stopped, onGoto, stopTick }) {
  const [view, setView] = useState("monitor");
  const [monitor, setMonitor] = useState({ events: [], lane_total: "0", lane_dropped: "0" });
  const [policy, setPolicy] = useState({ enabled: false, code: "0x00000000", pending: false });
  const [interceptors, setInterceptors] = useState([]);
  const [output, setOutput] = useState([]);
  const outputCursor = useRef("0");
  const [takeoverSeed, setTakeoverSeed] = useState(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    const [monitorResult, policyResult, inventoryResult, outputResult] = await Promise.all([
      api.exceptionMonitor(512),
      api.exceptionPolicyGet(),
      api.exceptionInventory(),
      api.scriptOutput(outputCursor.current, "1024"),
    ]);
    const errors = [];
    if (monitorResult.ok) setMonitor(monitorResult.value || { events: [] });
    else errors.push(monitorResult.error);
    if (policyResult.ok) setPolicy(policyResult.value || { enabled: false, code: "0x00000000", pending: false });
    else errors.push(policyResult.error);
    if (inventoryResult.ok) setInterceptors(Array.isArray(inventoryResult.value?.interceptors) ? inventoryResult.value.interceptors : []);
    else errors.push(inventoryResult.error);
    if (outputResult.ok) {
      const lines = Array.isArray(outputResult.value?.lines) ? outputResult.value.lines : [];
      outputCursor.current = String(outputResult.value?.next_cursor || outputCursor.current);
      if (lines.length) {
        setOutput((current) => [...current, ...lines].slice(-2048));
      }
    }
    setError(errors.filter(Boolean).join(" · "));
  }, []);

  useEffect(() => {
    let live = true;
    let timer = 0;
    const tick = async () => {
      if (!live) return;
      await refresh();
      if (live) timer = window.setTimeout(tick, 1500);
    };
    tick();
    return () => { live = false; window.clearTimeout(timer); };
  }, [refresh, stopTick]);

  async function savePolicy(enabled, code) {
    const normalized = normalizeCode(code);
    if (!normalized || busy) return false;
    setBusy(true);
    const result = await api.exceptionPolicySet(enabled, normalized);
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return false;
    }
    setPolicy(result.value || { enabled, code: normalized, pending: false });
    setError("");
    return true;
  }

  function prepareTakeover(event) {
    setTakeoverSeed({
      token: `${eventKey(event)}:${Date.now()}`,
      code: event.code,
      fromIp: event.from_ip || event.address,
      systemIp: event.system_to_ip || event.to_ip,
      finalIp: event.final_to_ip || event.to_ip,
    });
    setView("actions");
  }

  return (
    <section className="pba-panel pbe-panel">
      <header className="pba-panel-head">
        <div><b>异常</b><span>高优先级监控 · 同步处置</span></div>
        <button onClick={refresh} disabled={busy}>刷新</button>
      </header>
      <nav className="pbe-mode-tabs" aria-label="异常能力分类">
        <button className={view === "monitor" ? "active" : ""} onClick={() => setView("monitor")}>监控</button>
        <button className={view === "actions" ? "active" : ""} onClick={() => setView("actions")}>操作</button>
      </nav>
      {error && <div className="pba-error pbe-global-error" role="alert">{error}</div>}
      {view === "monitor" ? (
        <MonitorView monitor={monitor} stopped={stopped} onGoto={onGoto} onTakeover={prepareTakeover} />
      ) : (
        <ActionsView
          policy={policy}
          interceptors={interceptors}
          output={output}
          busy={busy}
          onSavePolicy={savePolicy}
          onRefresh={refresh}
          onError={setError}
          takeoverSeed={takeoverSeed}
          onConsumeTakeoverSeed={() => setTakeoverSeed(null)}
        />
      )}
      <footer className="pba-panel-foot">异常监控 · 暂停策略 · 同步接管</footer>
    </section>
  );
}

function MonitorView({ monitor, stopped, onGoto, onTakeover }) {
  const events = Array.isArray(monitor?.events) ? [...monitor.events].reverse() : [];
  const [source, setSource] = useState("all");
  const [selectedKey, setSelectedKey] = useState("");
  const visible = events.filter((event) => source === "all" || event.source === source);
  const selected = visible.find((event) => eventKey(event) === selectedKey) || null;

  if (selected) {
    return <ExceptionDetail event={selected} stopped={stopped} onGoto={onGoto} onTakeover={onTakeover} onBack={() => setSelectedKey("")} />;
  }

  return (
    <div className="pbe-body">
      <div className="pbe-monitor-bar">
        <div className="pbe-source-filter">
          <button className={source === "all" ? "active" : ""} onClick={() => setSource("all")}>全部</button>
          <button className={source === "target" ? "active" : ""} onClick={() => setSource("target")}>目标异常</button>
          <button className={source === "pin_internal" ? "active" : ""} onClick={() => setSource("pin_internal")}>Pin / Agent</button>
        </div>
        <span>保留通道 {monitor?.lane_total || "0"} · 丢弃 {monitor?.lane_dropped || "0"}</span>
      </div>
      {String(monitor?.lane_dropped || "0") !== "0" && (
        <div className="pbe-warning">日志不完整 · 丢弃 {monitor?.lane_dropped || "0"}</div>
      )}
      <div className="pbe-event-head">
        <span>来源</span><span>异常</span><span>线程</span><span>地址</span><span>代次</span>
      </div>
      <div className="pba-list pbe-event-list">
        {visible.length === 0 && (
          <div className="pba-empty"><b>无异常记录</b></div>
        )}
        {visible.map((event) => (
          <button key={eventKey(event)} className="pbe-event-row" onClick={() => setSelectedKey(eventKey(event))}>
            <span className={`pbe-source ${event.source}`}>{event.source === "target" ? "目标" : "Pin"}</span>
            <span><b>{eventTitle(event)}</b><small>{event.code}</small></span>
            <code>{event.thread_id}</code>
            <code>{normalizeAddress(event.address) || event.address || "—"}</code>
            <code>{event.generation || event.sequence || "—"}</code>
          </button>
        ))}
      </div>
    </div>
  );
}

function ExceptionDetail({ event, stopped, onGoto, onTakeover, onBack }) {
  const target = event.source === "target";
  const fromIp = normalizeAddress(event.from_ip || event.address);
  const systemIp = normalizeAddress(event.system_to_ip || event.to_ip);
  const finalIp = normalizeAddress(event.final_to_ip || event.to_ip);
  const route = exceptionRoute(event);
  return (
    <div className="pba-detail-scroll">
      <section className="pba-detail-section">
        <div className="pba-detail-title"><b>{target ? "目标异常详情" : "Pin / Agent 异常详情"}</b><button onClick={onBack}>返回列表</button></div>
        <div className="pba-detail-grid">
          <Detail label="来源" value={target ? "目标程序" : "Pin / Agent 内部"} />
          <Detail label="异常" value={eventTitle(event)} />
          <Detail label="异常码" value={String(event.code || "—")} mono />
          <Detail label="线程" value={`TID ${event.thread_id || "—"}`} />
          <Detail label="事件序号" value={event.sequence} />
          {target && <Detail label="异常代次" value={event.generation} />}
        </div>
      </section>
      {target ? (
        <section className="pba-detail-section">
          <div className="pba-detail-title"><b>异常流向与接管结果</b><i className={`pbe-route-badge ${route.kind}`}>{route.label}</i></div>
          <div className="pbe-route-flow">
            <RouteNode label="异常现场" address={fromIp} known={event.from_ip_known} onGoto={onGoto} />
            <span className="pbe-route-arrow">→</span>
            <RouteNode label="系统分发目标" address={systemIp} known={event.system_to_ip_known ?? event.to_ip_known} onGoto={onGoto} />
            <span className="pbe-route-arrow">→</span>
            <RouteNode label="最终执行去向" address={finalIp} known={event.final_to_ip_known ?? event.to_ip_known} onGoto={onGoto} emphasis />
          </div>
          <div className="pbe-disposition-grid">
            <Detail label="同步回调" value={event.disposition_available ? (event.interceptor_ran ? "已运行" : "系统处理") : "等待结果"} />
            <Detail label="上下文修改" value={event.takeover_applied ? "已应用" : "无修改"} />
            <Detail label="修改寄存器" value={Array.isArray(event.modified_registers) && event.modified_registers.length ? event.modified_registers.join(", ") : "—"} mono />
            <Detail label="处置事件" value={event.disposition_sequence || "—"} />
          </div>
          <div className="pbe-takeover-bar">
            <span>{stopped ? "目标已停止" : "同步接管"}</span>
            <button className="primary" onClick={() => onTakeover(event)}>为此异常创建接管</button>
          </div>
        </section>
      ) : (
        <section className="pba-detail-section">
          <div className="pba-detail-title"><b>内部诊断</b></div>
          <div className="pba-detail-grid">
            <Detail label="物理 IP" value={event.address} mono />
            <Detail label="异常地址" value={event.exception_address} mono />
            <Detail label="故障地址" value={event.fault_address_known ? event.fault_address : "不可用"} mono />
            <Detail label="访问类型" value={ACCESS_TYPES[String(event.access_type)] || event.access_type} />
            <Detail label="异常分类" value={PIN_EXCEPTION_CLASSES[String(event.exception_class)] || event.exception_class} />
          </div>
        </section>
      )}
    </div>
  );
}

function ActionsView({ policy, interceptors, output, busy, onSavePolicy, onRefresh, onError, takeoverSeed, onConsumeTakeoverSeed }) {
  const [enabled, setEnabled] = useState(Boolean(policy?.enabled));
  const [code, setCode] = useState(policy?.code || "0x00000000");
  const [selectedId, setSelectedId] = useState("");
  const [editorOpen, setEditorOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [scriptName, setScriptName] = useState("");
  const [source, setSource] = useState("");
  const [sourceMeta, setSourceMeta] = useState(null);
  const [sourceError, setSourceError] = useState("");
  const [saving, setSaving] = useState(false);
  const consumedTakeoverSeed = useRef("");
  const selected = interceptors.find((item) => String(item.id) === selectedId) || null;

  useEffect(() => {
    setEnabled(Boolean(policy?.enabled));
    setCode(policy?.code || "0x00000000");
  }, [policy?.enabled, policy?.code]);

  useEffect(() => {
    if (!selected || creating) return;
    let live = true;
    setSourceError("");
    api.scriptGet(selected.plugin).then((result) => {
      if (!live) return;
      if (!result.ok) {
        setSource("");
        setSourceMeta(null);
        setSourceError(result.error);
        return;
      }
      setScriptName(selected.plugin);
      setSource(String(result.value?.source || ""));
      setSourceMeta(result.value || null);
    });
    return () => { live = false; };
  }, [selected?.plugin, creating]);

  useEffect(() => {
    if (!takeoverSeed?.token || consumedTakeoverSeed.current === takeoverSeed.token) return;
    consumedTakeoverSeed.current = takeoverSeed.token;
    beginCreate(takeoverSeed);
    onConsumeTakeoverSeed?.();
  }, [takeoverSeed?.token, onConsumeTakeoverSeed]);

  function beginCreate(seed = null) {
    const name = `ui_exception_${Date.now().toString(36)}.py`;
    setCreating(true);
    setSelectedId("");
    setScriptName(name);
    setSource(exceptionTemplate(seed));
    setSourceMeta(null);
    setSourceError("");
    setEditorOpen(true);
  }

  async function saveSource(draft) {
    const nextName = String(draft?.name || "").trim();
    const nextSource = String(draft?.source || "");
    if (!nextName || !nextSource.trim() || saving) return false;
    setSaving(true);
    const result = creating
      ? await api.scriptInject(nextName, nextSource, "callback")
      : await api.scriptReplace(nextName, nextSource, sourceMeta?.kind || "callback");
    setSaving(false);
    if (!result.ok) {
      setSourceError(result.error);
      onError(result.error);
      return false;
    }
    setCreating(false);
    setEditorOpen(false);
    setScriptName(nextName);
    setSource(nextSource);
    await onRefresh();
    return true;
  }

  async function removeSelected() {
    if (!selected || !window.confirm(`卸载 ${selected.plugin} 及其异常回调？`)) return;
    const result = await api.scriptRemove(selected.plugin);
    if (!result.ok) {
      onError(result.error);
      return;
    }
    setSelectedId("");
    await onRefresh();
  }

  const pluginOutput = useMemo(
    () => output.filter((line) => line.plugin === selected?.plugin).slice(-80),
    [output, selected?.plugin],
  );

  return (
    <div className="pbe-body pbe-actions">
      <section className="pbe-action-section">
        <div className="pbe-section-title"><div><b>异常暂停策略</b><span>命中后请求调试器暂停</span></div><i className={policy?.pending ? "pending" : ""}>{policy?.pending ? "已有待处理异常" : "无待处理异常"}</i></div>
        <div className="pbe-policy-row">
          <label className="pbe-switch"><input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} /><span />启用异常断下</label>
          <label>异常码<input value={code} spellCheck="false" onChange={(event) => setCode(event.target.value)} placeholder="0 = 全部异常" /></label>
          <button className="primary" disabled={busy || !normalizeCode(code)} onClick={() => onSavePolicy(enabled, code)}>应用策略</button>
        </div>
        <div className="pbe-note">0 = 全部目标异常 · 异常边沿暂停</div>
      </section>

      <section className="pbe-action-section pbe-interceptor-section">
        <div className="pbe-section-title"><div><b>异常检查与接管</b><span>{interceptors.length} 个 exception.handle</span></div><button onClick={() => beginCreate()}>＋ 新建接管</button></div>
        <div className="pbe-takeover-explain">
          <span><b>from_registers</b> · 异常现场</span>
          <span><b>registers</b> · 目标上下文</span>
          <span><b>return None</b> · 系统处理</span>
          <span><b>return registers</b> · 修改去向</span>
        </div>
        {!selected ? (
          <div className="pbe-interceptor-list">
            {interceptors.length === 0 && <div className="pba-empty"><b>无异常接管</b></div>}
            {interceptors.map((item) => (
              <button key={`${item.plugin}:${item.id}`} className="pbe-interceptor-row" onClick={() => setSelectedId(String(item.id))}>
                <i className={`pba-dot ${item.owner === "ai" ? "ai" : "human"}`} />
                <span><b>{item.callback || "<callable>"}</b><small>{item.plugin}</small></span>
                <code>{codeFilterText(item.codes)}</code>
                <span>{item.thread_id == null ? "全部线程" : `TID ${item.thread_id}`}</span>
                <em className={interceptorState(item).kind}>{interceptorState(item).label}</em>
              </button>
            ))}
          </div>
        ) : (
          <div className="pbe-interceptor-detail">
            <div className="pba-detail-title"><b>{selected.callback}</b><button onClick={() => setSelectedId("")}>返回列表</button></div>
            <div className="pba-detail-grid compact">
              <Detail label="脚本" value={selected.plugin} mono />
              <Detail label="创建者" value={actorLabel(selected.created_by || selected.owner)} />
              <Detail label="回调说明" value={selected.description || "旧回调未提供说明"} />
              <Detail label="模式" value={selected.once ? "一次性" : "持续"} />
              <Detail label="线程" value={selected.thread_id == null ? "全部线程" : `TID ${selected.thread_id}`} />
              <Detail label="异常码" value={codeFilterText(selected.codes)} mono />
              <Detail label="最近代次" value={selected.last_generation || "0"} />
            </div>
            {selected.last_return != null && <ResultCard value={selected.last_return} generation={selected.last_generation} error={selected.last_error} />}
            {selected.last_error && <div className="pba-error">{selected.last_error}</div>}
            <div className="pbe-code-actions">
              <div><b>{scriptName || selected.plugin}</b><span>{source ? `${source.split(/\r?\n/).length} 行 Python` : "源码不可用"}</span></div>
              <button disabled={!selected.source_available} onClick={() => setEditorOpen(true)}>打开代码编辑器</button>
              <button className="danger" onClick={removeSelected}>卸载脚本</button>
            </div>
            {sourceError && <div className="pba-error">{sourceError}</div>}
            <div className="pbe-output">
              <div><b>脚本输出</b><span>{pluginOutput.length} 行</span></div>
              <pre>{pluginOutput.length ? pluginOutput.map((line) => `[${line.seq}] ${line.line}`).join("\n") : "暂无输出"}</pre>
            </div>
          </div>
        )}
      </section>
      <CallbackEditorDialog
        open={editorOpen}
        creating={creating}
        name={scriptName}
        source={source}
        meta={sourceMeta}
        error={sourceError}
        loading={false}
        saving={saving}
        readOnly={!creating && !selected?.source_available}
        callbackKind="异常接管"
        onClose={() => { setEditorOpen(false); if (creating) setCreating(false); }}
        onApply={saveSource}
      />
    </div>
  );
}

function Detail({ label, value, mono = false }) {
  return <div className="pba-detail-kv"><span>{label}</span><b className={mono ? "mono" : ""}>{value == null || value === "" ? "—" : value}</b></div>;
}

function AddressLine({ label, address, known, onGoto }) {
  const usable = Boolean(known && address && address !== "0x0");
  return <div className="pbe-address-line"><span>{label}</span><code>{known ? address : "不可用"}</code><button disabled={!usable} onClick={() => usable && onGoto(address)}>定位反汇编</button></div>;
}

function RouteNode({ label, address, known, onGoto, emphasis = false }) {
  const usable = Boolean(known && address && address !== "0x0");
  return (
    <button className={`pbe-route-node ${emphasis ? "emphasis" : ""}`} disabled={!usable} onClick={() => usable && onGoto(address)}>
      <span>{label}</span>
      <code>{known ? address : "不可用"}</code>
    </button>
  );
}

function ResultCard({ value, generation, error }) {
  const state = interceptorState({ last_return: value, last_error: error });
  return (
    <div className={`pbe-result ${state.kind}`}>
      <div><b>最近处置：{state.label}</b><span>代次 {generation || "—"}</span></div>
      <pre>{prettyReturn(value)}</pre>
    </div>
  );
}

function eventKey(event) {
  return `${event.source}:${event.sequence}`;
}

function eventTitle(event) {
  if (event.source === "target") return TARGET_EXCEPTION_NAMES[String(event.code || "").toLowerCase()] || "目标异常";
  return PIN_EXCEPTION_CLASSES[String(event.exception_class)] || "Pin 内部异常";
}

function normalizeCode(value) {
  const text = String(value ?? "").trim();
  if (!text) return null;
  try {
    const number = BigInt(text);
    if (number < 0n || number > 0xffffffffn) return null;
    return `0x${number.toString(16).padStart(8, "0")}`;
  } catch {
    return null;
  }
}

function codeFilterText(codes) {
  if (codes == null) return "全部异常";
  if (!Array.isArray(codes) || codes.length === 0) return "不匹配任何异常";
  return codes.join(", ");
}

function actorLabel(actor) {
  if (actor === "ai") return "AI";
  if (actor === "human") return "人工";
  return actor || "外部 / 未知";
}

function exceptionRoute(event) {
  if (!event.disposition_available) return { kind: "waiting", label: "等待处置结果" };
  if (event.takeover_applied) return { kind: "takeover", label: "回调已接管" };
  if (event.interceptor_ran) return { kind: "inspected", label: "已检查，交给系统" };
  return { kind: "system", label: "系统原生处理" };
}

function interceptorState(item) {
  if (item?.last_error) return { kind: "err", label: "处置失败" };
  if (item?.last_return == null || item.last_return === "") return { kind: "", label: "等待异常" };
  const rendered = String(item.last_return).trim();
  if (rendered === "None") return { kind: "pass", label: "检查后放行" };
  if (/['\"]registers['\"]\s*:/.test(rendered)) return { kind: "takeover", label: "已接管上下文" };
  return { kind: "ok", label: "已返回" };
}

function prettyReturn(value) {
  const rendered = String(value ?? "None").trim();
  if (rendered === "None") return "未修改目标上下文\n执行将继续前往系统分发目标";
  return rendered
    .replace(/^\{\s*['\"]registers['\"]\s*:\s*\{/, "写入目标上下文\n")
    .replace(/\}\s*\}\s*$/, "")
    .replace(/,\s*/g, "\n")
    .replace(/['\"]/g, "")
    .replace(/\s*:\s*/g, "  =  ");
}

function exceptionTemplate(seed = null) {
  const code = normalizeCode(seed?.code || "0xc0000005") || "0xc0000005";
  const fromIp = normalizeAddress(seed?.fromIp) || "未知";
  const systemIp = normalizeAddress(seed?.systemIp) || "未知";
  return `import pb

# 来源：现场 ${fromIp}，系统目标 ${systemIp}
# None = 全部异常
FILTER_CODES = [${code}]

# None = 系统处理；设置地址 = 接管
RECOVERY_IP = None

def on_exception(event):
    """异常接管。"""
    source = event["from_registers"]
    destination = event["registers"]
    ip_name = "rip" if "rip" in destination else "eip"
    pb.print(
        f"exception code=0x{event['code']:08x} "
        f"tid={event['tid']} "
        f"from=0x{source[ip_name]:x} system_to=0x{destination[ip_name]:x}"
    )

    if RECOVERY_IP is None:
        return None

    # 跳转到函数入口时，需自行构造目标 ABI 栈。
    return {"registers": {ip_name: RECOVERY_IP}}

def pb_init():
    pb.intercept(
        "exception.handle",
        on_exception,
        description="记录异常现场；RECOVERY_IP 非空时改写执行地址",
        codes=FILTER_CODES,
        once=False,
    )
`;
}
