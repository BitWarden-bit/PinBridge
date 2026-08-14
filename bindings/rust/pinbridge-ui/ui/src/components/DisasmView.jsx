import React, { useEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import { resolveInto } from "../resolve";

// Center disassembly view. Requests exactly as many rows as fit the panel
// (no inner scrollbar); mouse wheel pages forward/backward through the code.
// Branch/call targets are symbolized (agent RESOLVE) as trailing comments.
export default function DisasmView({ rows, rip, bpSet, onSetBp, onPage, onPageUp }) {
  const t = useT();
  const ripRef = useRef(null);
  const boxRef = useRef(null);
  const [names, setNames] = useState({});
  useEffect(() => {
    if (ripRef.current) ripRef.current.scrollIntoView({ block: "center" });
  }, [rip, rows]);
  useEffect(() => {
    const targets = rows.filter((r) => r.target && r.target !== "0x0").map((r) => r.target);
    if (!targets.length) return;
    let live = true;
    resolveInto(targets).then((c) => {
      if (live) setNames(Object.fromEntries(targets.map((x) => [x, c.get(x)])));
    });
    return () => {
      live = false;
    };
  }, [rows]);

  function onWheel(e) {
    if (!rows.length) return;
    if (e.deltaY > 0) {
      const last = rows[rows.length - 1];
      const size = last.bytes.length / 2;
      onPage("0x" + (parseInt(last.address, 16) + size).toString(16));
    } else {
      onPageUp(rows[0].address);
    }
  }

  return (
    <div id="disasm" ref={boxRef} onWheel={onWheel}>
      <div className="panel-title" style={{ padding: "6px 10px 0" }}>{t("disassembly")}</div>
      <table>
        <tbody>
          {rows.map((r) => {
            const addrNum = parseInt(r.address, 16);
            const cls = [
              addrNum === rip ? "rip" : "",
              bpSet.has(r.address) ? "bp-row" : "",
              r.kind ? "k" + r.kind : "",
            ].join(" ").trim();
            return (
              <tr key={r.address} className={cls} ref={addrNum === rip ? ripRef : null}>
                <td className="c-addr" title="点击下断点" onClick={() => onSetBp(r.address)}>
                  {r.address}
                </td>
                <td className="c-bytes">{r.bytes}</td>
                <td className="c-text">
                  {r.text}
                  {r.target && names[r.target] ? (
                    <span className="cmt"> ; {names[r.target]}</span>
                  ) : null}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
