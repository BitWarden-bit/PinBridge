import React, { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { callWithError } from "../api";
import { IconChip, IconGo } from "../icons";
import { useT } from "../i18n";

// Launch screen shown when no backend session is running: pick a target
// executable and start pin + agent for it (x64dbg "open file" style).
export default function LaunchScreen({ onLaunched }) {
  const t = useT();
  const [target, setTarget] = useState(() => localStorage.getItem("pb-target") || "");
  const [pin, setPin] = useState(() => localStorage.getItem("pb-pin") || "");
  const [entryBp, setEntryBp] = useState(() => localStorage.getItem("pb-entry-bp") !== "0");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function browse(setter, title, storeKey) {
    const picked = await open({ title, filters: [{ name: "Executable", extensions: ["exe"] }] });
    if (picked) {
      setter(picked);
      localStorage.setItem(storeKey, picked);
    }
  }

  async function launch() {
    if (!target || busy) return;
    setBusy(true);
    setError("");
    localStorage.setItem("pb-target", target);
    if (pin) localStorage.setItem("pb-pin", pin);
    localStorage.setItem("pb-entry-bp", entryBp ? "1" : "0");
    const result = await callWithError("cmd_launch", { target, pin: pin || null, entryBp });
    setBusy(false);
    if (!result.ok) {
      setError(result.error); // show the real reason (pin path, agent DLL, spawn…)
      return;
    }
    onLaunched(target);
  }

  return (
    <div id="launch">
      <div id="launch-card">
        <div id="launch-brand"><IconChip size={34} /> <span>PinBridge</span></div>
        <div className="panel-title">{t("platform")}</div>
        <div id="launch-row">
          <input
            value={target}
            placeholder={t("targetPlaceholder")}
            onChange={(e) => setTarget(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && launch()}
          />
          <button onClick={() => browse(setTarget, "Target", "pb-target")}>{t("browse")}</button>
        </div>
        <div id="launch-row">
          <input
            value={pin}
            placeholder={t("pinPlaceholder")}
            onChange={(e) => setPin(e.target.value)}
          />
          <button onClick={() => browse(setPin, "pin.exe", "pb-pin")}>{t("browse")}</button>
        </div>
        <label style={{ display: "flex", gap: 6, alignItems: "center", margin: "4px 0 10px", cursor: "pointer" }}>
          <input
            type="checkbox"
            checked={entryBp}
            onChange={(e) => setEntryBp(e.target.checked)}
            style={{ width: "auto", margin: 0 }}
          />
          {t("entryBp")}
        </label>
        <button className="primary" id="launch-btn" onClick={launch} disabled={busy || !target}>
          <IconGo /> {busy ? t("launching") : t("launch")}
        </button>
        {error && <div className="err" style={{ marginTop: 8 }}>{error}</div>}
        <div className="launch-hint">{t("launchHint")}</div>
      </div>
    </div>
  );
}
