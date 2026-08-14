import React from "react";
import { api } from "../api";
import { getLang, setLang, useT } from "../i18n";
import { IconChip, IconFollow, IconGo, IconPause, IconPlay, IconStepInto, IconStepOver } from "../icons";

export default function Toolbar({ onStop, onFollowRip, onGoto, tid, status, target, onKillSession }) {
  const t = useT();
  return (
    <div id="toolbar">
      <span id="brand"><IconChip /> PinBridge</span>
      <span className="sep" />
      <button title={target || "current target"} onClick={onKillSession}>
        ⏏ {t("target")}
      </button>
      <span className="sep" />
      <button className="primary" onClick={() => api.control("resume")}>
        <IconPlay /> {t("continue")}
      </button>
      <button onClick={async () => { await api.control("stop"); onStop(); }}>
        <IconPause /> {t("pause")}
      </button>
      <span className="sep" />
      <button onClick={async () => { await api.step(tid, false); onStop(); }}>
        <IconStepInto /> {t("stepInto")}
      </button>
      <button onClick={async () => { await api.step(tid, true); onStop(); }}>
        <IconStepOver /> {t("stepOver")}
      </button>
      <span className="sep" />
      <button onClick={onFollowRip}><IconFollow /> {t("followRip")}</button>
      <GotoBox onGoto={onGoto} go={t("go")} placeholder={t("addressPlaceholder")} />
      <button id="lang-toggle" onClick={() => setLang(getLang() === "en" ? "zh" : "en")}>
        {getLang() === "en" ? "中" : "EN"}
      </button>
      <span id="statustext" className={status.err ? "err" : "ok"}>{status.text}</span>
    </div>
  );
}

function GotoBox({ onGoto, go, placeholder }) {
  const [value, setValue] = React.useState("");
  return (
    <>
      <input
        value={value}
        placeholder={placeholder}
        style={{ width: 170 }}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && onGoto(value)}
      />
      <button onClick={() => onGoto(value)}><IconGo /> {go}</button>
    </>
  );
}
