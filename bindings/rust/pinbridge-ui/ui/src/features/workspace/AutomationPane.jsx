import React, { useEffect, useState } from "react";
import { api } from "../../api";
import { normalizeAddress } from "../../address";
import { MemoryMapTab } from "../../components/MemoryLayoutTabs";
import CallbackEditorDialog from "./CallbackEditorDialog";
import ExceptionPanel from "./ExceptionPanel";
import HookPanel from "./HookPanel";
import ModuleScriptPanel from "./ModuleScriptPanel";
import TracePanel from "./TracePanel";

// Right automation plane. Every feature here follows the same template: a
// FeaturePanel shell (title + one simple primary button), a complete item
// list below it, and only real Agent data — no simulated entries.
const FEATURES = [
  { id: "bps", name: "断点", ready: true },
  { id: "memory", name: "内存布局", ready: true },
  { id: "exceptions", name: "异常", ready: true },
  { id: "hooks", name: "Hook", ready: true },
  { id: "trace", name: "Trace", ready: true },
  { id: "scripts", name: "模块脚本", ready: true },
  { id: "ai", name: "AI 活动", ready: true },
];

export default function AutomationPane({ rip, stopped, hitAddr, bps, onGoto, onRefreshBreakpoints, activities, onRefreshActivities, stopTick }) {
  const [feature, setFeature] = useState("bps");
  return (
    <div className="pba-pane">
      <nav className="pba-feature-tabs" aria-label="自动化功能">
        {FEATURES.map((item) => (
          <button
            key={item.id}
            className={feature === item.id ? "active" : ""}
            disabled={!item.ready}
            title={item.ready ? item.name : `${item.name} · 待接入`}
            onClick={() => setFeature(item.id)}
          >
            {item.name}
          </button>
        ))}
      </nav>
      {feature === "bps" && (
        <BreakpointPanel rip={rip} stopped={stopped} hitAddr={hitAddr} bps={bps} onGoto={onGoto} onRefresh={onRefreshBreakpoints} />
      )}
      {feature === "ai" && (
        <ActivityPanel activities={activities || []} onRefresh={onRefreshActivities} />
      )}
      {feature === "memory" && (
        <section className="pbl-right-host">
          <MemoryMapTab stopTick={stopTick} onGoto={onGoto} />
        </section>
      )}
      {feature === "exceptions" && (
        <ExceptionPanel stopped={stopped} stopTick={stopTick} onGoto={onGoto} />
      )}
      {feature === "hooks" && (
        <HookPanel rip={rip} stopped={stopped} stopTick={stopTick} onGoto={onGoto} activities={activities || []} />
      )}
      {feature === "trace" && (
        <TracePanel stopped={stopped} stopTick={stopTick} onGoto={onGoto} />
      )}
      {feature === "scripts" && (
        <ModuleScriptPanel stopTick={stopTick} />
      )}
    </div>
  );
}

// Template shell: panel header carries the title and the single primary
// action; the body keeps the complete management surface for the feature.
function FeaturePanel({ title, hint, primary, children }) {
  return (
    <section className="pba-panel">
      <header className="pba-panel-head">
        <div><b>{title}</b><span>{hint}</span></div>
        {primary}
      </header>
      {children}
    </section>
  );
}

function BreakpointPanel({ rip, stopped, hitAddr, bps, onGoto, onRefresh }) {
  const [address, setAddress] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [selectedId, setSelectedId] = useState(null);
  const current = normalizeAddress(rip);
  const canUseCurrent = stopped && current && current !== "0x0";
  const hit = stopped ? normalizeAddress(hitAddr) : null;
  const selected = bps.find((bp) => String(bp.id) === selectedId) || null;

  async function add(target) {
    const normalized = normalizeAddress(target);
    if (!normalized || busy) return;
    setBusy(true);
    setError("");
    const result = await api.bpSetResult(normalized);
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setAddress("");
    await onRefresh?.();
  }

  async function remove(id) {
    setError("");
    const result = await api.bpRemoveResult(id);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setSelectedId(null);
    await onRefresh?.();
  }

  if (selected) {
    return (
      <BreakpointDetail
        breakpoint={selected}
        hit={hit}
        onBack={() => setSelectedId(null)}
        onGoto={onGoto}
        onRefresh={onRefresh}
      />
    );
  }

  return (
    <FeaturePanel
      title="断点管理"
      hint={`${bps.length} 个 · 实时快照`}
      primary={
        <button
          className="primary"
          disabled={!canUseCurrent || busy}
          title={canUseCurrent ? `在 ${current} 下断` : "目标停止后可用"}
          onClick={() => add(current)}
        >
          ＋ 在当前地址下断
        </button>
      }
    >
      <div className="pba-add-row">
        <input
          value={address}
          placeholder={canUseCurrent ? current : "0x 地址"}
          spellCheck="false"
          onChange={(event) => setAddress(event.target.value)}
          onKeyDown={(event) => event.key === "Enter" && add(address)}
        />
        <button disabled={busy || !normalizeAddress(address)} onClick={() => add(address)}>下断</button>
      </div>
      {error && <div className="pba-error" role="alert">{error}</div>}
      <div className="pba-list">
        {bps.length === 0 && (
          <div className="pba-empty"><b>无断点</b><span>输入地址或使用当前 RIP</span></div>
        )}
        {bps.map((bp) => (
          <BreakpointRow key={bp.id} bp={bp} hit={hit} onGoto={onGoto} onRemove={remove} onSelect={() => setSelectedId(String(bp.id))} />
        ))}
      </div>
      <footer className="pba-panel-foot">普通断点 · 回调断点</footer>
    </FeaturePanel>
  );
}

function BreakpointRow({ bp, hit, onGoto, onRemove, onSelect }) {
  const address = normalizeAddress(bp.address) || bp.address;
  const isHit = hit != null && address === hit;
  const callbacks = Array.isArray(bp.callbacks) ? bp.callbacks : [];
  const owners = Array.isArray(bp.plain_owners) ? bp.plain_owners : [];
  const tone = breakpointTone(owners, callbacks);
  const latest = latestCallback(callbacks);
  const protectedByCallback = callbacks.length > 0;
  return (
    <div className={`pba-row pba-bp-row ${isHit ? "hit" : ""}`} role="button" tabIndex={0} onClick={onSelect} onKeyDown={(event) => event.key === "Enter" && onSelect()}>
      <i className={`pba-dot ${tone}`} title={ownerSummary(owners, callbacks)} />
      <button className="pba-addr" title="在左侧反汇编中定位" onClick={(event) => { event.stopPropagation(); onGoto(address); }}>{address}</button>
      <span className="pba-kind">{breakpointKind(bp.kind, callbacks)}</span>
      <span className="pba-meta">#{bp.id} · 命中 {bp.hits}</span>
      {callbacks.length > 0 && <span className="pba-callback-name">{callbacks[0].callback}{callbacks.length > 1 ? ` +${callbacks.length - 1}` : ""}</span>}
      {latest?.last_error && <span className="pba-return err">异常</span>}
      {!latest?.last_error && latest?.last_action && <span className="pba-return">→ {actionLabel(latest.last_action)}</span>}
      {isHit && <span className="pba-hit-tag">当前命中</span>}
      <button
        className="pba-del"
        disabled={protectedByCallback}
        title={protectedByCallback ? "先卸载回调脚本" : "删除物理断点"}
        onClick={(event) => { event.stopPropagation(); onRemove(bp.id); }}
      >删除</button>
    </div>
  );
}

function BreakpointDetail({ breakpoint, hit, onBack, onGoto, onRefresh }) {
  const callbacks = Array.isArray(breakpoint.callbacks) ? breakpoint.callbacks : [];
  const owners = Array.isArray(breakpoint.plain_owners) ? breakpoint.plain_owners : [];
  const address = normalizeAddress(breakpoint.address) || breakpoint.address;
  const [selectedPlugin, setSelectedPlugin] = useState(callbacks[0]?.plugin || "");
  const [source, setSource] = useState("");
  const [scriptName, setScriptName] = useState("");
  const [sourceMeta, setSourceMeta] = useState(null);
  const [sourceError, setSourceError] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [creating, setCreating] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const active = callbacks.find((callback) => callback.plugin === selectedPlugin) || callbacks[0] || null;

  useEffect(() => {
    if (!active || creating) return;
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
    });
    return () => { live = false; };
  }, [active?.plugin, creating]);

  function beginCallback() {
    const safeId = String(breakpoint.id).replace(/[^0-9a-z_-]/gi, "_");
    setCreating(true);
    setSelectedPlugin("");
    setScriptName(`ui_breakpoint_${safeId}.py`);
    setSource(callbackTemplate(address));
    setSourceMeta(null);
    setSourceError("");
    setEditorOpen(true);
  }

  async function saveSource(draft) {
    const nextName = String(draft?.name || "").trim();
    const nextSource = String(draft?.source || "");
    if (!nextName || !nextSource.trim() || saving) return false;
    setSaving(true);
    setSourceError("");
    const result = creating
      ? await api.scriptInject(nextName, nextSource, "callback")
      : await api.scriptReplace(nextName, nextSource, sourceMeta?.kind || "callback");
    setSaving(false);
    if (!result.ok) {
      setSourceError(result.error);
      return false;
    }
    setScriptName(nextName);
    setSource(nextSource);
    setCreating(false);
    setSourceMeta(result.value || null);
    setSelectedPlugin(nextName);
    setEditorOpen(false);
    await onRefresh?.();
    return true;
  }

  function closeEditor() {
    setEditorOpen(false);
    if (creating) {
      setCreating(false);
      setSelectedPlugin(callbacks[0]?.plugin || "");
    }
  }

  const isCurrentHit = hit != null && address === hit;
  return (
    <FeaturePanel
      title="断点详情"
      hint={`${address} · #${breakpoint.id}`}
      primary={<button onClick={onBack}>返回列表</button>}
    >
      <div className="pba-detail-scroll">
        <section className="pba-detail-section">
          <div className="pba-detail-title"><b>基本信息</b><button onClick={() => onGoto(address)}>在反汇编中定位</button></div>
          <div className="pba-detail-grid">
            <Detail label="类型" value={breakpointKind(breakpoint.kind, callbacks)} />
            <Detail label="创建来源" value={ownerSummary(owners, callbacks)} />
            <Detail label="命中次数" value={String(breakpoint.hits ?? "0")} />
            <Detail label="状态" value={isCurrentHit ? "当前命中" : "等待命中"} />
          </div>
        </section>

        <section className="pba-detail-section">
          <div className="pba-detail-title"><b>回调绑定</b><button onClick={beginCallback}>＋ 添加回调</button></div>
          {callbacks.length === 0 && !creating && <div className="pba-detail-empty">普通断点 · 命中时停止</div>}
          {callbacks.length > 0 && (
            <div className="pba-binding-tabs">
              {callbacks.map((callback) => (
                <button key={`${callback.plugin}:${callback.callback}`} className={!creating && active?.plugin === callback.plugin ? "active" : ""} onClick={() => { setCreating(false); setSelectedPlugin(callback.plugin); }}>
                  {callback.plugin} · {callback.callback}
                </button>
              ))}
            </div>
          )}
          {active && !creating && (
            <div className="pba-detail-grid compact">
              <Detail label="函数" value={active.callback || "—"} mono />
              <Detail label="作者" value={actorLabel(active.created_by || active.owner)} />
              <Detail label="回调说明" value={active.description || "后端未提供（旧回调）"} />
              <Detail label="模式" value={active.once ? "一次性" : "持续"} />
              <Detail label="线程" value={active.thread_id == null ? "全部线程" : `TID ${active.thread_id}`} />
              <Detail label="控制动作" value={active.last_action ? actionLabel(active.last_action) : "未命中"} />
              <Detail label="停止代次" value={active.last_stop_generation || "0"} />
            </div>
          )}
          {active?.last_return != null && !creating && (
            <CallbackReturnCard
              value={active.last_return}
              action={active.last_action}
              generation={active.last_stop_generation}
            />
          )}
          {active?.last_error && !creating && <div className="pba-error" role="alert">{active.last_error}</div>}
        </section>

        {(active || creating) && (
          <section className="pba-detail-section pba-code-section">
            <div className="pba-detail-title"><b>{creating ? "新回调代码" : "回调代码"}</b><span>{loading ? "读取中…" : sourceMeta ? `generation ${sourceMeta.generation || "—"} · ${actorLabel(sourceMeta.modified_by)} 修改` : ""}</span></div>
            {sourceError && <div className="pba-error" role="alert">{sourceError}</div>}
            <div className="pba-code-summary">
              <div className="pba-code-file"><i>PY</i><div><b>{scriptName || active?.plugin || "未命名脚本"}</b><span>{loading ? "读取源码…" : `${source ? source.split(/\r?\n/).length : 0} 行 · 插件脚本`}</span></div></div>
              <div className="pba-code-capabilities"><span>VS Code 编辑内核</span><span>Python 高亮</span><span>API 索引</span><span>点击插入</span></div>
              <button
                className="primary"
                disabled={loading || (!creating && !active?.source_available)}
                onClick={() => setEditorOpen(true)}
              >{creating ? "打开编辑器创建" : "在代码编辑器中打开"}</button>
            </div>
          </section>
        )}
      </div>
      <footer className="pba-panel-foot">脚本 · 绑定 · 返回 · 错误</footer>
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
        onClose={closeEditor}
        onApply={saveSource}
      />
    </FeaturePanel>
  );
}

function Detail({ label, value, mono = false }) {
  return <div className="pba-detail-kv"><span>{label}</span><b className={mono ? "mono" : ""}>{value || "—"}</b></div>;
}

function CallbackReturnCard({ value, action, generation }) {
  const [copied, setCopied] = useState(false);
  const raw = String(value);
  const formatted = formatPythonRepr(raw);
  const shape = returnShape(raw);

  async function copyReturn() {
    try {
      await navigator.clipboard.writeText(raw);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      setCopied(false);
    }
  }

  return (
    <section className="pba-return-card" aria-label="回调完整返回值">
      <header>
        <div><i>↳</i><span><b>回调返回</b><small>Python 返回值</small></span></div>
        <span className="pba-return-shape">{shape}</span>
        {action && <span className={`pba-return-action ${action}`}>{actionLabel(action)}</span>}
        <button onClick={copyReturn}>{copied ? "已复制" : "复制"}</button>
      </header>
      <pre>{formatted}</pre>
      <footer><span>Python repr</span><span>{raw.length.toLocaleString("zh-CN")} 字符</span><span>停止代次 {generation || "—"}</span></footer>
    </section>
  );
}

function returnShape(value) {
  const first = value.trim()[0];
  if (first === "{") return "DICT";
  if (first === "[") return "LIST";
  if (first === "(") return "TUPLE";
  if (first === "'" || first === '"') return "STRING";
  return "VALUE";
}

function formatPythonRepr(value) {
  const text = value.trim();
  if (!text || !"{[(".includes(text[0])) return text;
  let output = "";
  let indent = 0;
  let quote = "";
  let escaped = false;
  const appendIndent = () => { output += "  ".repeat(Math.max(0, indent)); };

  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (quote) {
      output += char;
      if (escaped) escaped = false;
      else if (char === "\\") escaped = true;
      else if (char === quote) quote = "";
      continue;
    }
    if (char === "'" || char === '"') {
      quote = char;
      output += char;
      continue;
    }
    if ("{[(".includes(char)) {
      output += char;
      if (!"}])".includes(text[index + 1] || "")) {
        indent += 1;
        output += "\n";
        appendIndent();
      }
      continue;
    }
    if ("}])".includes(char)) {
      if (!"{[(".includes(text[index - 1] || "")) {
        indent = Math.max(0, indent - 1);
        output = output.replace(/[ \t]+$/, "");
        output += "\n";
        appendIndent();
      }
      output += char;
      continue;
    }
    if (char === ",") {
      output += ",\n";
      appendIndent();
      while (text[index + 1] === " ") index += 1;
      continue;
    }
    if (char === ":") {
      output += ": ";
      while (text[index + 1] === " ") index += 1;
      continue;
    }
    output += char;
  }
  return output;
}

function callbackTemplate(address) {
  return `import pb\n\nADDRESS = ${address}\n\ndef on_hit(event):\n    # stay / resume / step_into / step_over\n    return "stay"\n\ndef pb_init():\n    pb.breakpoint(\n        ADDRESS,\n        on_hit,\n        description="断点命中后保持停止",\n    )\n`;
}

function breakpointKind(kind, callbacks) {
  if (kind === "mixed") return "普通 + 回调";
  if (kind === "callback" || callbacks.length > 0) return "回调断点";
  if (kind === "external") return "外部断点";
  return "普通断点";
}

function breakpointTone(owners, callbacks) {
  if (owners.includes("ai") || callbacks.some((item) => item.owner === "ai")) return "ai";
  if (owners.includes("human") || callbacks.some((item) => item.owner === "human")) return "human";
  return callbacks.length > 0 ? "strategy" : "human";
}

function ownerSummary(owners, callbacks) {
  const values = new Set();
  owners.forEach((owner) => values.add(actorLabel(owner)));
  callbacks.forEach((callback) => values.add(callback.owner === "ai" ? "AI 回调" : callback.owner === "human" ? "人工回调" : `策略 ${callback.plugin}`));
  return values.size > 0 ? Array.from(values).join("、") : "外部/未知";
}

function latestCallback(callbacks) {
  return [...callbacks].sort((left, right) => Number(right.last_stop_generation || 0) - Number(left.last_stop_generation || 0))[0] || null;
}

function actionLabel(action) {
  return ({ stay: "保持停止", resume: "继续运行", step_into: "单步进入", step_over: "单步越过" })[action] || action || "—";
}

// AI / human operation timeline from the real Hub activity journal.
function ActivityPanel({ activities, onRefresh }) {
  const [selectedId, setSelectedId] = useState(null);
  const selected = activities.find((item) => (item.operation_id || "") === selectedId) || null;
  return (
    <FeaturePanel
      title="AI 活动"
      hint={`${activities.length} 条 · 每 3.5s 刷新`}
      primary={<button onClick={onRefresh} title="立即刷新">刷新</button>}
    >
      <div className="pba-list">
        {activities.length === 0 && (
          <div className="pba-empty"><b>无活动记录</b></div>
        )}
        {activities.map((item) => (
          <ActivityRow
            key={item.operation_id || item.started_at_ms}
            activity={item}
            selected={selectedId === (item.operation_id || "")}
            onSelect={() => setSelectedId((current) => current === (item.operation_id || "") ? null : (item.operation_id || ""))}
          />
        ))}
      </div>
      {selected && <ActivityDetail activity={selected} onClose={() => setSelectedId(null)} />}
    </FeaturePanel>
  );
}

function ActivityRow({ activity, selected, onSelect }) {
  const inFlight = !activity.completed_at_ms && activity.outcome === "in_progress";
  const tone = inFlight ? "wait" : activity.outcome === "ok" ? "ok" : activity.outcome ? "err" : "";
  return (
    <button className={`pba-act-row ${selected ? "active" : ""}`} onClick={onSelect}>
      <span className="pba-act-time">{formatTime(activity.started_at_ms)}</span>
      <span className={`pba-act-actor ${actorClass(activity.actor)}`}>{actorLabel(activity.actor)}</span>
      <code className="pba-act-tool">{activity.action || "operation"}</code>
      <span className="pba-act-purpose">{activity.purpose || "未提供目的"}</span>
      <span className={`pba-act-result ${tone}`}>{inFlight ? "进行中" : activity.outcome || "—"}</span>
    </button>
  );
}

function ActivityDetail({ activity, onClose }) {
  return (
    <div className="pba-act-detail">
      <div className="pba-act-detail-head">
        <b>{activity.action || "operation"}</b>
        <code>{activity.operation_id || "—"}</code>
        <button onClick={onClose}>收起</button>
      </div>
      <div className="pba-act-kv"><span>目的</span><b>{activity.purpose || "—"}</b></div>
      <div className="pba-act-kv"><span>参数</span><b className="mono">{resourceText(activity.resource_refs) || "—"}</b></div>
      <div className="pba-act-kv"><span>父操作</span><b className="mono">{activity.parent_operation_id || "—"}</b></div>
      <div className="pba-act-kv"><span>开始</span><b>{formatTime(activity.started_at_ms)}</b></div>
      <div className="pba-act-kv"><span>耗时</span><b>{elapsedMs(activity.started_at_ms, activity.completed_at_ms)}</b></div>
      <div className="pba-act-kv"><span>结果</span><b>{activity.outcome || "—"}</b></div>
    </div>
  );
}

function actorClass(actor) {
  const value = String(actor || "").toLowerCase();
  if (value === "ai") return "ai";
  if (value === "human" || value === "operator") return "human";
  return value === "target" ? "target" : "";
}

function actorLabel(actor) {
  const value = String(actor || "").toLowerCase();
  if (value === "ai") return "AI";
  if (value === "human" || value === "operator") return "人工";
  return value === "target" ? "目标" : actor || "系统";
}

function formatTime(value) {
  if (!/^\d+$/.test(String(value || ""))) return "—";
  const date = new Date(Number(value));
  return Number.isNaN(date.getTime()) ? "—" : date.toLocaleTimeString("zh-CN", { hour12: false });
}

function elapsedMs(start, completed) {
  if (!/^\d+$/.test(String(start || ""))) return "—";
  if (!/^\d+$/.test(String(completed || ""))) return "进行中";
  try {
    return `${BigInt(completed) - BigInt(start)} ms`;
  } catch {
    return "—";
  }
}

function resourceText(value) {
  if (!value || typeof value !== "object") return value == null ? "" : String(value);
  if (Array.isArray(value)) return value.map(resourceText).filter(Boolean).join(", ");
  return Object.entries(value)
    .filter(([, item]) => item != null && item !== "")
    .map(([key, item]) => `${key}=${typeof item === "object" ? resourceText(item) : item}`)
    .join(" · ");
}
