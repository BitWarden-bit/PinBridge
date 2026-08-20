import React, { useState } from "react";

export function RvaRangeEditor({
  ranges,
  onChange,
  disabled = false,
  maxRanges = 16,
  compact = false,
}) {
  const rows = Array.isArray(ranges) && ranges.length ? ranges : [{ begin: "0x0", end: "0x1000" }];

  function update(index, field, value) {
    onChange(rows.map((range, row) => row === index ? { ...range, [field]: value } : range));
  }

  function add() {
    if (disabled || rows.length >= maxRanges) return;
    const previousEnd = rows.at(-1)?.end || "0x0";
    onChange([...rows, { begin: previousEnd, end: nextPageBoundary(previousEnd) }]);
  }

  function remove(index) {
    if (disabled || rows.length <= 1) return;
    onChange(rows.filter((_, row) => row !== index));
  }

  return (
    <div className={`pbs-range-editor ${compact ? "compact" : ""}`}>
      <div className="pbs-range-columns">
        <span>起始 RVA</span><span>结束 RVA</span><span>长度</span><span />
      </div>
      <div className="pbs-range-rows">
        {rows.map((range, index) => {
          const summary = rangeSummary(range);
          return (
            <div className={`pbs-range-row ${summary.error ? "invalid" : ""}`} key={index}>
              <span className="pbs-range-index">{String(index + 1).padStart(2, "0")}</span>
              <label><span>起始 RVA</span><input disabled={disabled} value={range.begin} onChange={(event) => update(index, "begin", event.target.value)} spellCheck="false" placeholder="0x0" /></label>
              <i>→</i>
              <label><span>结束 RVA</span><input disabled={disabled} value={range.end} onChange={(event) => update(index, "end", event.target.value)} spellCheck="false" placeholder="0x1000" /></label>
              <code title={summary.error || summary.size}>{summary.error ? "无效" : summary.size}</code>
              <button disabled={disabled || rows.length <= 1} title="删除此范围" onClick={() => remove(index)}>×</button>
            </div>
          );
        })}
      </div>
      {maxRanges > 1 && (
        <div className="pbs-range-footer">
          <button disabled={disabled || rows.length >= maxRanges} onClick={add}>＋ 添加范围</button>
          <span>{rows.length} / {maxRanges} · 半开区间</span>
        </div>
      )}
    </div>
  );
}

export function ValueTokenEditor({
  values,
  onChange,
  disabled = false,
  maxValues = 64,
  placeholder = "输入后按 Enter",
  normalize = (value) => value.trim(),
}) {
  const [draft, setDraft] = useState("");
  const items = Array.isArray(values) ? values : [];

  function commit(source = draft) {
    if (disabled) return;
    const incoming = String(source || "").split(/[\s,;]+/).map(normalize).filter(Boolean);
    if (!incoming.length) return;
    const seen = new Set(items.map((item) => String(item).toLowerCase()));
    const next = [...items];
    incoming.forEach((item) => {
      const key = String(item).toLowerCase();
      if (next.length < maxValues && !seen.has(key)) {
        seen.add(key);
        next.push(item);
      }
    });
    onChange(next);
    setDraft("");
  }

  function remove(index) {
    if (!disabled) onChange(items.filter((_, item) => item !== index));
  }

  return (
    <div className={`pbs-token-editor ${disabled ? "disabled" : ""}`}>
      {items.map((item, index) => (
        <span key={`${item}:${index}`}><code>{item}</code><button disabled={disabled} title={`删除 ${item}`} onClick={() => remove(index)}>×</button></span>
      ))}
      <input
        disabled={disabled || items.length >= maxValues}
        value={draft}
        placeholder={items.length ? "继续添加…" : placeholder}
        spellCheck="false"
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => commit()}
        onPaste={(event) => {
          const text = event.clipboardData?.getData("text");
          if (text && /[\s,;]/.test(text)) {
            event.preventDefault();
            commit(text);
          }
        }}
        onKeyDown={(event) => {
          if (["Enter", ",", ";"].includes(event.key)) {
            event.preventDefault();
            commit();
          } else if (event.key === "Backspace" && !draft && items.length) {
            remove(items.length - 1);
          }
        }}
      />
      <em>{items.length}/{maxValues}</em>
    </div>
  );
}

function rangeSummary(range) {
  const begin = parseUnsigned(range?.begin);
  const end = parseUnsigned(range?.end);
  if (begin == null || end == null) return { error: "请输入十六进制或十进制 RVA" };
  if (end <= begin) return { error: "结束 RVA 必须大于起始 RVA" };
  return { size: formatHex(end - begin) };
}

function nextPageBoundary(value) {
  const begin = parseUnsigned(value);
  return begin == null ? "0x1000" : formatHex(begin + 0x1000n);
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

function formatHex(value) {
  return `0x${value.toString(16).toUpperCase()}`;
}
