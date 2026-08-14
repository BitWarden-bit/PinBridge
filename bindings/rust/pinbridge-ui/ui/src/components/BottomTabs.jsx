import React, { useEffect, useState } from "react";
import { api } from "../api";
import { getSnapshot, subscribe } from "../store";
import { useT } from "../i18n";
import { resolveInto } from "../resolve";

export default function BottomTabs({ tid, stopTick, onGoto }) {
  const t = useT();
  const [tab, setTab] = useState("events");
  const tabs = [["mem", t("memory")], ["stack", t("stack")], ["bps", t("breakpoints")], ["mods", t("modules")], ["events", t("events")]];
  return (
    <div id="bottom">
      <div id="tabs">
        {tabs.map(([key, label]) => (
          <div key={key} className={"tab" + (tab === key ? " active" : "")} onClick={() => setTab(key)}>
            {label}
          </div>
        ))}
      </div>
      <div id="tabbody">
        {tab === "mem" && <MemoryTab />}
        {tab === "stack" && <StackTab tid={tid} stopTick={stopTick} onGoto={onGoto} />}
        {tab === "bps" && <BpsTab />}
        {tab === "mods" && <ModsTab onGoto={onGoto} />}
        {tab === "events" && <EventsTab />}
      </div>
    </div>
  );
}

function MemoryTab() {
  const t = useT();
  const [addr, setAddr] = useState("");
  const [size, setSize] = useState("256");
  const [hex, setHex] = useState("");
  const [edit, setEdit] = useState(null); // { off, value } byte being edited
  const base = parseInt(addr, 16) || 0;

  const refresh = async (a = addr, s = size) => {
    const data = await api.readMem(a, parseInt(s) || 256);
    if (data !== undefined) setHex(data);
  };

  function startEdit(off, current) {
    setEdit({ off, value: current });
  }
  async function commitEdit() {
    if (!edit) return;
    const value = edit.value.replace(/\s+/g, "").toLowerCase();
    // batch: any even-length run overwrites consecutive bytes from the edit point
    if (/^([0-9a-f]{2})+$/.test(value)) {
      const at = "0x" + (base + edit.off / 2).toString(16);
      const written = await api.writeMem(at, value);
      if (written !== undefined) {
        const next = hex.substr(0, edit.off) + value + hex.substr(edit.off + value.length);
        setHex(next);
      }
    }
    setEdit(null);
  }

  const rows = [];
  for (let off = 0; off < hex.length; off += 32) {
    rows.push({ off, chunk: hex.substr(off, 32) });
  }
  return (
    <div>
      <div style={{ marginBottom: 8, display: "flex", gap: 6 }}>
        <input value={addr} onChange={(e) => setAddr(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && refresh()} placeholder="0x address" />
        <input value={size} onChange={(e) => setSize(e.target.value)} style={{ width: 70 }} />
        <button onClick={() => refresh()}>{t("read")}</button>
        <span style={{ color: "var(--dim)", alignSelf: "center" }}>点击字节直接输入,可连续覆盖多个字节,回车写入</span>
      </div>
      <table><tbody>
        {rows.map((r) => {
          const bytes = [];
          for (let i = 0; i < r.chunk.length; i += 2) bytes.push(r.chunk.substr(i, 2));
          let ascii = "";
          bytes.forEach((b) => {
            const v = parseInt(b, 16);
            ascii += v >= 32 && v < 127 ? String.fromCharCode(v) : "·";
          });
          return (
            <tr key={r.off}>
              <td style={{ color: "var(--addr)" }}>0x{(base + r.off / 2).toString(16)}</td>
              <td className="c-bytes">
                {bytes.map((b, bi) => {
                  const off = r.off + bi * 2;
                  const gap = (bi + 1) % 4 === 0 ? "  " : " ";
                  if (edit && edit.off === off) {
                    return (
                      <input key={bi} className="byte-edit" autoFocus value={edit.value}
                        style={{ width: Math.max(22, edit.value.length * 7 + 8) }}
                        onChange={(e) => setEdit({ off, value: e.target.value })}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") commitEdit();
                          if (e.key === "Escape") setEdit(null);
                        }}
                        onBlur={commitEdit} />
                    );
                  }
                  return (
                    <span key={bi} className="byte-cell" onClick={() => startEdit(off, b)}>
                      {b + gap}
                    </span>
                  );
                })}
              </td>
              <td style={{ color: "var(--dim)" }}>{ascii}</td>
            </tr>
          );
        })}
      </tbody></table>
    </div>
  );
}

function StackTab({ tid, stopTick, onGoto }) {
  const t = useT();
  const [rows, setRows] = useState([]);
  const [names, setNames] = useState({});
  const load = async () => {
    if (tid == null) return;
    const regs = await api.context(tid);
    if (!regs) return;
    const rsp = parseInt(regs.find((r) => r.reg === 6).value, 16);
    const hex = await api.readMem("0x" + rsp.toString(16), 256);
    if (hex === undefined) return;
    const out = [];
    for (let off = 0; off < hex.length; off += 16) {
      const le = hex.substr(off, 16).match(/../g).reverse().join("");
      out.push({ addr: "0x" + (rsp + off / 2).toString(16), value: "0x" + le });
    }
    setRows(out);
    // symbolize whatever points into a module (return addresses etc.)
    resolveInto(out.map((r) => r.value)).then((c) =>
      setNames(Object.fromEntries(out.map((r) => [r.value, c.get(r.value)])))
    );
  };
  useEffect(() => { load(); }, [tid, stopTick]);
  if (tid == null) return <div style={{ color: "var(--dim)" }}>{t("pauseFirst")}</div>;
  return (
    <div>
      <div style={{ marginBottom: 6 }}>
        <button onClick={load}>{t("read")}</button>
      </div>
      <table><tbody>
        {rows.map((r) => (
          <tr key={r.addr}>
            <td style={{ color: "var(--dim)" }}>{r.addr}</td>
            <td style={{ color: "var(--addr)", cursor: "pointer" }} onClick={() => onGoto(r.value)}>{r.value}</td>
            <td className="cmt">{names[r.value] || ""}</td>
          </tr>
        ))}
      </tbody></table>
    </div>
  );
}

// Breakpoints come from the 4Hz snapshot (source of truth is the agent), so
// hits update live and markers survive view switches.
function BpsTab() {
  const t = useT();
  const [, force] = React.useReducer((x) => x + 1, 0);
  useEffect(() => subscribe(force), []);
  const { bps, stopped, hitAddr } = getSnapshot();
  return (
    <div>
      <table><tbody>
        <tr style={{ color: "var(--dim)" }}><td></td><td>ID</td><td>{t("addressPlaceholder")}</td><td>{t("hits")}</td><td></td></tr>
        {bps.map((b) => (
          <tr key={b.id}>
            <td style={{ color: "var(--err)" }}>●</td><td>#{b.id}</td>
            <td style={{ color: "var(--addr)" }}>{b.address}</td><td>{b.hits}</td>
            <td><button onClick={() => api.bpRemove(b.id)}>{t("delete")}</button></td>
          </tr>
        ))}
      </tbody></table>
      {stopped && <div style={{ marginTop: 6, color: "var(--dim)" }}>{t("stopped")} @ {hitAddr}</div>}
    </div>
  );
}

function ModsTab({ onGoto }) {
  const t = useT();
  const [mods, setMods] = useState([]);
  useEffect(() => { api.modules().then((m) => m && setMods(m)); }, []);
  return (
    <table><tbody>
      <tr style={{ color: "var(--dim)" }}><td></td><td>{t("base")}</td><td>{t("end")}</td><td>{t("name")}</td></tr>
      {mods.map((m) => (
        <tr key={m.low}>
          <td>{m.main ? "★" : ""}</td>
          <td style={{ color: "var(--addr)", cursor: "pointer" }} onClick={() => onGoto(m.low)}>{m.low}</td>
          <td>{m.high}</td><td>{m.name.split("\\").pop()}</td>
        </tr>
      ))}
    </tbody></table>
  );
}

function EventsTab() {
  const [, force] = React.useReducer((x) => x + 1, 0);
  const [names, setNames] = useState({});
  useEffect(() => subscribe(force), []);
  const { events } = getSnapshot();
  useEffect(() => {
    const addrs = [];
    events.forEach((e) => {
      addrs.push(e.address);
      if (e.kind === 4) addrs.push(e.arg0); // branch target
    });
    resolveInto(addrs).then((c) =>
      setNames(Object.fromEntries(addrs.map((a) => [a, c.get(a)])))
    );
  }, [events]);
  return (
    <table><tbody>
      {events.map((e) => (
        <tr key={e.sequence}><td>{eventLine(e, names)}</td></tr>
      ))}
    </tbody></table>
  );
}

function eventLine(e, names = {}) {
  const ip = names[e.address] ? `${e.address} ${names[e.address]}` : e.address;
  const tgt = names[e.arg0] ? `${e.arg0} ${names[e.arg0]}` : e.arg0;
  switch (e.kind) {
    case 1: return `#${e.sequence} Hook   Tid=${e.thread_id} Ip=${ip} Rcx=${e.arg0} Rdx=${e.arg1}`;
    case 2: return `#${e.sequence} Mem    Tid=${e.thread_id} Ip=${ip} Ea=${e.arg0} Size=${e.arg1} Acc=${e.arg2}`;
    case 4: return `#${e.sequence} Branch Tid=${e.thread_id} Ip=${ip} Target=${tgt} Taken=${e.arg1}`;
    case 5: return `#${e.sequence} Sys    Tid=${e.thread_id} Nr=${e.arg0} Phase=${e.arg1} A0=${e.arg2}`;
    case 6: return `#${e.sequence} Ctx    Tid=${e.thread_id} Reason=${e.arg0} Info=${e.arg1} Ip=${e.arg2}`;
    default: return `#${e.sequence} Exec   Tid=${e.thread_id} Ip=${ip} Size=${e.arg0}`;
  }
}
