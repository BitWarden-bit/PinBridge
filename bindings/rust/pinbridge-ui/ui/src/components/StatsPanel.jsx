import React, { useEffect, useRef } from "react";
import { getSnapshot, subscribe } from "../store";
import { useT } from "../i18n";

// Rate chart is drawn imperatively on a canvas (never through React's render
// loop); the rest of the panel is cheap text.
export default function StatsPanel() {
  const canvasRef = useRef(null);
  const [, force] = React.useReducer((x) => x + 1, 0);

  useEffect(() => subscribe(force), []);

  useEffect(() => {
    const c = canvasRef.current;
    if (!c) return;
    const { rateHistory } = getSnapshot();
    const dpr = window.devicePixelRatio || 1;
    c.width = c.clientWidth * dpr;
    c.height = c.clientHeight * dpr;
    const ctx = c.getContext("2d");
    ctx.scale(dpr, dpr);
    const w = c.clientWidth, h = c.clientHeight;
    ctx.clearRect(0, 0, w, h);
    if (rateHistory.length < 2) return;
    const chartValues = rateHistory.map((value) => {
      const number = Number(value);
      return Number.isSafeInteger(number) && number >= 0 ? number : 0;
    });
    const max = Math.max(...chartValues, 1);
    ctx.strokeStyle = "#f2f2f2";
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    chartValues.forEach((v, i) => {
      const x = (i / (159 - 1)) * w;
      const y = h - (v / max) * (h - 6) - 3;
      i === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
    });
    ctx.stroke();
  });

  const s = getSnapshot();
  const t = useT();
  // Counters cross the Tauri boundary as decimal strings to preserve u64
  // precision. Convert only for bounded display/chart arithmetic.
  const fmt = (n) => {
    const text = String(n ?? "0");
    const value = Number(text);
    return Number.isSafeInteger(value) ? value.toLocaleString("en-US") : text;
  };
  const minDecimal = (a, b) => {
    try {
      const left = BigInt(String(a ?? "0"));
      const right = BigInt(String(b ?? "0"));
      return (left < right ? left : right).toString();
    } catch {
      return "0";
    }
  };
  const names = ["Hook", "Mem", "Exec", "Branch", "Sys", "Ctx"];
  return (
    <div id="stats">
      <div className="panel-title">
        {t("eventRate")} <b style={{ color: "var(--fg)" }}>{fmt(s.rate)}</b>/s
      </div>
      <canvas id="chart" ref={canvasRef} />
      <div id="kinds" style={{ marginTop: 6 }}>
        {names.map((n, i) => (
          <span className="kv" key={n}>{n} <b>{fmt(s.kinds[i] || 0)}</b></span>
        ))}
      </div>
      <div id="ringtext" style={{ color: "var(--dim)", marginTop: 4 }}>
        ring {fmt(minDecimal(s.total, s.capacity))}/{fmt(s.capacity)}
      </div>
    </div>
  );
}
