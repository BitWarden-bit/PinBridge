import React, { useEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import { resolveInto } from "../resolve";
import { addAddress, normalizeAddress } from "../address";

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
      const next = addAddress(last.address, size);
      if (next) onPage(next);
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
            const address = normalizeAddress(r.address) || r.address;
            const cls = [
              address === rip ? "rip" : "",
              bpSet.has(address) ? "bp-row" : "",
              r.kind ? "k" + r.kind : "",
            ].join(" ").trim();
            return (
              <tr key={r.address} className={cls} ref={address === rip ? ripRef : null}>
                <td className="c-addr" title={t("clickBreakpoint")} onClick={() => onSetBp(r.address)}>
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
