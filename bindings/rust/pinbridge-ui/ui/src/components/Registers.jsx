import React from "react";
import { api } from "../api";
import { useT } from "../i18n";

const REG_ORDER = [10, 7, 9, 8, 4, 3, 5, 6, 11, 12, 13, 14, 15, 16, 17, 18, 26, 25];
const REG_NAMES = {
  3: "rdi", 4: "rsi", 5: "rbp", 6: "rsp", 7: "rbx", 8: "rdx", 9: "rcx", 10: "rax",
  11: "r8", 12: "r9", 13: "r10", 14: "r11", 15: "r12", 16: "r13", 17: "r14",
  18: "r15", 26: "rip", 25: "rflags",
};

// RFLAGS (25, x64) / EFLAGS (57, x86) ship as one raw value; the individual
// status bits are what a human actually reads, so they are parsed out here.
const FLAGS_IDS = [25, 57];
const FLAG_BITS = [
  ["CF", 0], ["PF", 2], ["AF", 4], ["ZF", 6], ["SF", 7],
  ["TF", 8], ["IF", 9], ["DF", 10], ["OF", 11],
];

function parseFlags(text) {
  try {
    return BigInt(text);
  } catch {
    return null;
  }
}

export default function Registers({ tid, regs, onChanged }) {
  const t = useT();
  async function edit(reg, oldv) {
    const value = window.prompt(`${REG_NAMES[reg]} — ${t("newValue")}:`, oldv);
    if (!value) return;
    await api.setreg(tid, reg, value);
    onChanged();
  }
  async function toggleFlag(flagsId, current, bit) {
    if (tid == null) return;
    const next = current ^ (1n << BigInt(bit));
    await api.setreg(tid, flagsId, "0x" + next.toString(16));
    onChanged();
  }
  const map = {};
  regs.forEach((r) => (map[r.reg] = r.value));
  const flagsId = FLAGS_IDS.find((id) => map[id] != null) ?? null;
  const flags = flagsId != null ? parseFlags(map[flagsId]) : null;
  const half = Math.ceil(REG_ORDER.length / 2);
  const columns = [REG_ORDER.slice(0, half), REG_ORDER.slice(half)];
  return (
    <div id="regs">
      <div className="panel-title">{t("registers")} {tid != null ? `— Tid ${tid}` : t("pauseToSelect")}</div>
      <div className="reg-cols">
        {columns.map((col, ci) => (
          <table key={ci}>
            <tbody>
              {col.map((id) => {
                const v = map[id] || "0x0";
                return (
                  <tr key={id}>
                    <td className="name">{REG_NAMES[id]}</td>
                    <td className="val" onClick={() => tid && edit(id, v)}>{v}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ))}
      </div>
      {flags != null && (
        <div className="flags-row">
          {FLAG_BITS.map(([name, bit]) => {
            const on = ((flags >> BigInt(bit)) & 1n) === 1n;
            return (
              <span
                key={name}
                className={on ? "on" : ""}
                title={tid != null ? `${name} = ${on ? 1 : 0} · ${t("flagToggleHint")}` : `${name} = ${on ? 1 : 0}`}
                onClick={() => toggleFlag(flagsId, flags, bit)}
              >
                {name}
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}
