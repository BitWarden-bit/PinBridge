import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api";
import { useT } from "../../i18n";

const ACTIVITY_TYPES = [
  "operation", "breakpoint_set", "breakpoint_hit", "hook_set", "hook_hit",
  "script_inject", "script_replace", "script_start", "script_stop", "script_remove", "script_error", "context_read", "context_write",
  "memory_read", "memory_write", "target_pause", "target_resume",
];

export default function AiWorkbench({ snapshot, session, control, onControl, onServiceState, onHandoff, onTakeover, actionBusy }) {
  const t = useT();
  const [service, setService] = useState({ state: "checking", error: "" });
  const [controlStatus, setControlStatus] = useState(control);
  const [runtimeStatus, setRuntimeStatus] = useState(null);
  const [activities, setActivities] = useState([]);
  const [selected, setSelected] = useState(null);
  const [filter, setFilter] = useState("all");
  const refreshGeneration = useRef(0);

  const refresh = useCallback(async () => {
    const generation = ++refreshGeneration.current;
    const current = () => generation === refreshGeneration.current;
    const controlResult = await api.ai.controlStatus();
    if (!current()) return;
    const controlAvailable = controlResult.ok && controlResult.value?.mode && controlResult.value.ai_adapter_available !== false;
    if (!controlAvailable) {
      const nextService = {
        state: "offline",
        error: controlResult.value?.ai_adapter_available === false
          ? "AI control adapter unavailable"
          : controlResult.error || "unknown control state",
      };
      setService(nextService);
      onServiceState?.("offline");
      setControlStatus(controlResult.value?.mode ? controlResult.value : null);
      onControl?.(controlResult.value?.mode ? controlResult.value : null);
    } else {
      const next = controlResult.value;
      setControlStatus(next);
      onControl?.(next);
      onServiceState?.("connected");
      setService({ state: "connected", error: "" });
    }
    const sessionResult = await api.ai.sessionStatus();
    if (!current()) return;
    if (sessionResult.ok) setRuntimeStatus(sessionResult.value || null);
    else setRuntimeStatus(null);
    const activityResult = await api.ai.activityList({ limit: "100" });
    if (!current()) return;
    if (activityResult.ok) {
      const nextActivities = Array.isArray(activityResult.value?.activities) ? activityResult.value.activities : [];
      setActivities(nextActivities);
    } else if (!controlResult.ok) {
      setService((previous) => ({ ...previous, error: previous.error || activityResult.error }));
    }
  }, [onControl, onServiceState]);

  useEffect(() => {
    refresh();
    const timer = window.setInterval(refresh, 3500);
    return () => {
      window.clearInterval(timer);
      refreshGeneration.current += 1;
    };
  }, [refresh]);

  useEffect(() => setControlStatus(control), [control]);

  const filtered = useMemo(() => filter === "all" ? activities : activities.filter((item) => activityType(item) === filter), [activities, filter]);
  const active = selected || filtered[0] || null;
  const runtimeSession = runtimeStatus?.session || runtimeStatus || {};
  const runtimeAgent = runtimeStatus?.agent || {};
  const runtimeField = (keys) => first(runtimeSession, keys) ?? first(runtimeAgent, keys);
  const targetRunning = runningState(runtimeStatus, snapshot);

  return (
    <main className="ai-workbench">
      <section className="ai-header">
        <div>
          <div className="eyebrow">{t("aiDebugDesk")}</div>
          <h1>{t("aiLedMode")}</h1>
          <p>{t("aiDeskSubtitle")}</p>
        </div>
        <div className="ai-service-state">
          <span className={`service-dot ${service.state}`} />
          <span>{service.state === "connected" ? t("controlServiceConnected") : service.state === "checking" ? t("checkingService") : t("controlServiceOffline")}</span>
          {service.error && <span className="service-error" title={service.error}>{t("readOnlyFallback")}</span>}
        </div>
      </section>

      <section className="ai-status-grid" aria-label={t("sessionStatus")}>
        <StatusCell label={t("session")} value={runtimeField(["session_id", "sessionId", "id"]) || "—"} mono />
        <StatusCell label={t("targetPid")} value={runtimeField(["target_pid", "targetPid", "pid"]) ?? snapshot?.pid ?? "—"} mono />
        <StatusCell label={t("targetState")} value={stateLabel(runtimeStatus, snapshot, t)} tone={targetRunning === undefined ? "unknown" : targetRunning ? "running" : "stopped"} />
        <StatusCell label={t("stopAddress")} value={first(runtimeStatus, ["stop_address", "stopAddress", "stopped_at", "stoppedAt"]) || snapshot?.hitAddr || "—"} mono />
        <StatusCell label={t("stopThread")} value={first(runtimeStatus, ["thread", "thread_id", "threadId"]) ?? snapshot?.hitTid ?? "—"} mono />
        <StatusCell label={t("stopReason")} value={first(runtimeStatus, ["stop_reason", "stopReason", "reason"]) || "—"} />
        <StatusCell label={t("currentOperation")} value={first(runtimeStatus, ["current_operation", "currentOperation", "operation"]) || "—"} />
        <StatusCell label={t("currentScript")} value={first(runtimeStatus, ["current_script", "currentScript", "script"]) || "—"} mono />
      </section>

      <div className="ai-body">
        <section className="activity-panel">
          <div className="panel-heading">
            <div><div className="panel-title">{t("activityTimeline")}</div><div className="panel-caption">{t("activityStructuredHint")}</div></div>
            <select value={filter} onChange={(event) => setFilter(event.target.value)} aria-label={t("filterActivity")}>
              <option value="all">{t("allActivity")}</option>
              {ACTIVITY_TYPES.map((type) => <option key={type} value={type}>{type}</option>)}
            </select>
          </div>
          <div className="activity-list">
            {filtered.length === 0 ? <EmptyActivity service={service.state} t={t} /> : filtered.map((item, index) => (
              <ActivityCard key={item.operation_id || `${activityType(item)}-${index}`} activity={item} selected={active === item} onClick={() => setSelected(item)} t={t} />
            ))}
          </div>
        </section>
        <AssetSidebar activities={activities} t={t} />
        <ActivityDetail activity={active} t={t} />
      </div>
    </main>
  );
}

function StatusCell({ label, value, mono, tone }) {
  return <div className="status-cell"><span>{label}</span><strong className={`${mono ? "mono" : ""} ${tone || ""}`}>{String(value)}</strong></div>;
}

function ActivityCard({ activity, selected, onClick, t }) {
  const type = activityType(activity);
  const outcome = first(activity, ["outcome", "result", "status"]) || "—";
  return <button className={`activity-card ${selected ? "selected" : ""}`} onClick={onClick}>
    <div className="activity-card-top"><span className={`activity-type type-${type.split("_")[0]}`}>{type}</span><span className="activity-time">{formatTimeMs(first(activity, ["started_at_ms", "startedAtMs"]))}</span></div>
    <div className="activity-purpose">{first(activity, ["purpose", "description", "summary"]) || t("purposeUnavailable")}</div>
    <div className="activity-meta"><span>{t("actor")}: <b>{first(activity, ["actor", "actor_type"]) || "—"}</b></span><span>{t("outcome")}: <b>{String(outcome)}</b></span></div>
    <div className="activity-meta secondary"><span>{t("resource")}: <b className="mono">{resourceSummary(activity.resource_refs) || "—"}</b></span><span>{t("parent")}: <b className="mono">{activity.parent_operation_id || "—"}</b></span></div>
  </button>;
}

function AssetSidebar({ activities, t }) {
  const assets = {
    breakpoints: activities.filter((item) => /^breakpoint_/.test(item.action)),
    hooks: activities.filter((item) => /^hook_/.test(item.action)),
    scripts: activities.filter((item) => /^script_/.test(item.action)),
    collections: activities.filter((item) => /^(memory|context)_/.test(item.action)),
  };
  const groups = [
    ["breakpoints", t("breakpoints"), assets.breakpoints], ["hooks", t("hooks"), assets.hooks],
    ["scripts", t("dynamicScripts"), assets.scripts], ["collections", t("collectionTasks"), assets.collections],
  ];
  return <aside className="asset-sidebar"><div className="panel-title">{t("assetOverview")}</div>{groups.map(([key, label, values]) => <div className="asset-group" key={key}><div className="asset-group-head"><span>{label}</span><b>{values.length}</b></div>{values.length > 0 ? values.slice(0, 4).map((value, index) => <div className="asset-item" key={value.operation_id || index}>{resourceSummary(value.resource_refs) || value.action || "—"}</div>) : <div className="asset-empty">{t("notAvailable")}</div>}</div>)}</aside>;
}

function ActivityDetail({ activity, t }) {
  if (!activity) return <aside className="detail-panel"><div className="panel-title">{t("activityDetails")}</div><div className="detail-empty">{t("selectActivity")}</div></aside>;
  const fields = [
    [t("operationId"), activity.operation_id], [t("type"), activityType(activity)], [t("actor"), activity.actor], [t("purpose"), activity.purpose],
    [t("startedAt"), activity.started_at_ms], [t("completedAt"), activity.completed_at_ms],
    [t("outcome"), activity.outcome], [t("resource"), activity.resource_refs], [t("parent"), activity.parent_operation_id],
    [t("before"), first(activity, ["before", "before_value", "beforeValue"])], [t("after"), first(activity, ["after", "after_value", "afterValue"])],
  ];
  return <aside className="detail-panel"><div className="panel-title">{t("activityDetails")}</div><div className="detail-fields">{fields.map(([label, value]) => <div className="detail-row" key={label}><span>{label}</span><b className={typeof value === "string" && /^0x/i.test(value) ? "mono" : ""}>{value == null || value === "" ? "—" : structuredText(value)}</b></div>)}</div></aside>;
}

function EmptyActivity({ service, t }) {
  return <div className="activity-empty"><div className="empty-mark">{service === "offline" ? "×" : "○"}</div><strong>{service === "offline" ? t("activityServiceUnavailable") : t("noActivityYet")}</strong><span>{service === "offline" ? t("activityServiceHint") : t("activityEmptyHint")}</span></div>;
}

function first(object, keys) {
  if (!object) return undefined;
  for (const key of keys) if (object[key] !== undefined && object[key] !== null && object[key] !== "") return object[key];
  return undefined;
}
function activityType(activity) { return String(activity?.action || "operation"); }
function resourceSummary(value) { return value == null ? "" : structuredText(value, 0, 2); }
function structuredText(value, depth = 0, maxItems = 8) {
  if (value == null) return "—";
  if (typeof value !== "object") return String(value);
  if (depth >= 3) return "…";
  if (Array.isArray(value)) return value.slice(0, maxItems).map((item) => structuredText(item, depth + 1, maxItems)).join(", ");
  return Object.entries(value).slice(0, maxItems).map(([key, item]) => `${key}: ${structuredText(item, depth + 1, maxItems)}`).join("; ");
}
function formatTimeMs(value) {
  if (value == null || value === "") return "—";
  try {
    const text = String(value);
    if (!/^-?\d+$/.test(text)) return "—";
    const millis = BigInt(text);
    const limit = 8640000000000000n;
    if (millis < -limit || millis > limit) return "—";
    const date = new Date(Number(millis));
    return Number.isNaN(date.getTime()) ? "—" : date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  } catch { return "—"; }
}
function runningState(status, snapshot) {
  const value = first(status, ["running", "is_running"])
    ?? first(status?.session, ["running", "is_running"])
    ?? first(status?.agent, ["running", "is_running"]);
  if (value !== undefined) return !!value;
  return snapshot?.connected ? !snapshot.stopped : undefined;
}
function stateLabel(status, snapshot, t) {
  const running = runningState(status, snapshot);
  return running === undefined ? t("targetStateUnknown") : running ? t("running") : t("stopped");
}
