import React from "react";
import { useT } from "../../i18n";

export default function AiEmptyState({ onManual }) {
  const t = useT();
  return <main className="ai-empty-shell"><div className="ai-empty-card"><div className="eyebrow">{t("aiDebugDesk")}</div><h1>{t("aiNeedsSession")}</h1><p>{t("aiNeedsSessionHint")}</p><button className="primary" onClick={onManual}>{t("returnToManual")}</button></div></main>;
}
