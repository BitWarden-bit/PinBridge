import React from "react";
import { IconChip } from "../../icons";
import { useT } from "../../i18n";

export default function ModeBar({ viewMode, onModeChange, control, serviceState, onHandoff, onTakeover, actionBusy, actionState, hasSession }) {
  const t = useT();
  const controlMode = control?.mode;
  const controlKnown = Boolean(controlMode);
  const aiMode = controlMode === "ai_read_only" || controlMode === "ai_assist" || controlMode === "ai_autonomous" || controlMode === "automation_paused";
  const owner = controlMode === "manual" ? "manual" : aiMode ? "ai" : "unknown";
  const permission = permissionForMode(controlMode, t);
  const handoffDisabled = actionBusy || !hasSession || serviceState === "offline" || controlMode !== "manual";
  // Keep takeover available as an emergency attempt when control status is
  // unknown/disconnected; the trusted backend remains the final authority.
  const takeoverDisabled = actionBusy || !hasSession || (!aiMode && controlKnown);
  return (
    <div className="modebar">
      <div className="modebar-brand"><IconChip size={18} /><span>PinBridge</span></div>
      <div className="mode-switch" role="tablist" aria-label={t("controlMode")}>
        <button className={viewMode === "manual" ? "mode-tab active" : "mode-tab"} onClick={() => onModeChange("manual")} role="tab" aria-selected={viewMode === "manual"}>
          {t("manualMode")}
        </button>
        <button className={viewMode === "ai" ? "mode-tab active ai" : "mode-tab ai"} onClick={() => onModeChange("ai")} role="tab" aria-selected={viewMode === "ai"}>
          {t("aiLedMode")}
        </button>
      </div>
      <div className="modebar-control">
        <span className="modebar-label">{t("controlOwner")}</span>
        <span className={`owner-pill ${owner === "ai" ? "ai" : owner === "manual" ? "manual" : "unknown"}`}>
          {owner === "ai" ? (controlMode === "automation_paused" ? t("aiPaused") : t("aiControl")) : owner === "manual" ? t("manualControl") : t("unknownState")}
        </span>
        <span className="permission-pill">{t("permission")}: {permission}</span>
      </div>
      <div className="modebar-actions">
        {actionState?.message && <span className={`handoff-state ${actionState.kind || ""}`}>{actionState.message}</span>}
        <button className="handoff-button" onClick={onHandoff} disabled={handoffDisabled}>
          {actionBusy && actionState?.action === "handoff" ? t("handingOff") : t("handoffToAi")}
        </button>
        <button className="takeover-button" onClick={onTakeover} disabled={takeoverDisabled}>
          {actionBusy && actionState?.action === "takeover" ? t("takingOver") : t("takeoverNow")}
        </button>
      </div>
    </div>
  );
}

function permissionForMode(mode, t) {
  if (mode === "ai_read_only") return t("permissionReadOnly");
  if (mode === "ai_assist") return t("permissionAssist");
  if (mode === "ai_autonomous") return t("permissionAutonomous");
  if (mode === "automation_paused") return t("permissionPaused");
  return mode === "manual" ? t("permissionManual") : t("unknownState");
}
