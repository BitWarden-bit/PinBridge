import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../../api";
import CallbackEditorDialog from "./CallbackEditorDialog";
import defaultModuleSource from "../../../../module-templates/analysis_module.py?raw";

const OUTPUT_LIMIT = 1024;

export default function ModuleScriptPanel({ stopTick }) {
  const [scripts, setScripts] = useState([]);
  const [selectedName, setSelectedName] = useState("");
  const [source, setSource] = useState("");
  const [sourceMeta, setSourceMeta] = useState(null);
  const [output, setOutput] = useState([]);
  const [error, setError] = useState("");
  const [loadingSource, setLoadingSource] = useState(false);
  const [busy, setBusy] = useState(false);
  const [creating, setCreating] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const [draftName, setDraftName] = useState("");
  const outputCursor = useRef("0");

  const modules = useMemo(
    () => scripts.filter((script) => script.kind !== "callback"),
    [scripts],
  );
  const selected = modules.find((script) => script.name === selectedName) || null;
  const selectedOutput = useMemo(
    () => output.filter((line) => line.plugin === selectedName).slice(-300),
    [output, selectedName],
  );

  const refresh = useCallback(async () => {
    const [listResult, outputResult] = await Promise.all([
      api.scriptList(),
      api.scriptOutput(outputCursor.current, String(OUTPUT_LIMIT)),
    ]);
    const errors = [];
    if (listResult.ok) {
      setScripts(Array.isArray(listResult.value) ? listResult.value : []);
    } else {
      errors.push(listResult.error);
    }
    if (outputResult.ok) {
      const lines = Array.isArray(outputResult.value?.lines) ? outputResult.value.lines : [];
      outputCursor.current = String(outputResult.value?.next_cursor || outputCursor.current);
      if (lines.length) setOutput((current) => [...current, ...lines].slice(-4096));
    } else {
      errors.push(outputResult.error);
    }
    setError(errors.filter(Boolean).join(" · "));
  }, []);

  useEffect(() => {
    let live = true;
    let timer = 0;
    const tick = async () => {
      if (!live) return;
      await refresh();
      if (live) timer = window.setTimeout(tick, 1800);
    };
    tick();
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [refresh, stopTick]);

  useEffect(() => {
    if (modules.length === 0) {
      setSelectedName("");
      return;
    }
    if (!modules.some((script) => script.name === selectedName)) {
      setSelectedName(modules[0].name);
    }
  }, [modules, selectedName]);

  useEffect(() => {
    if (!selected || creating) {
      if (!selected) {
        setSource("");
        setSourceMeta(null);
      }
      return undefined;
    }
    let live = true;
    setLoadingSource(true);
    api.scriptGet(selected.name).then((result) => {
      if (!live) return;
      setLoadingSource(false);
      if (!result.ok) {
        setSource("");
        setSourceMeta({ ...selected, source_available: false });
        return;
      }
      setSource(String(result.value?.source || ""));
      setSourceMeta(result.value || selected);
    });
    return () => { live = false; };
  }, [selected?.name, selected?.generation, creating]);

  function beginCreate() {
    const suffix = Date.now().toString(36);
    setCreating(true);
    setDraftName(`analysis_module_${suffix}.py`);
    setSource(defaultModuleSource);
    setSourceMeta(null);
    setEditorOpen(true);
    setError("");
  }

  function beginEdit() {
    if (!selected || !sourceMeta?.source_available) return;
    setCreating(false);
    setDraftName(selected.name);
    setEditorOpen(true);
  }

  async function saveSource(draft) {
    if (busy) return false;
    const name = String(draft?.name || "").trim();
    const nextSource = String(draft?.source || "");
    if (!name || !nextSource.trim()) return false;
    setBusy(true);
    setError("");
    const result = creating
      ? await api.scriptInject(name, nextSource, "module")
      : await api.scriptReplace(name, nextSource, "module");
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return false;
    }
    setCreating(false);
    setEditorOpen(false);
    setSelectedName(name);
    setSource(nextSource);
    setSourceMeta(result.value || null);
    await refresh();
    return true;
  }

  async function changeRuntime(action) {
    if (!selected || busy) return;
    setBusy(true);
    setError("");
    const result = action === "start"
      ? await api.scriptStart(selected.name)
      : await api.scriptStop(selected.name);
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    await refresh();
  }

  async function removeSelected() {
    if (!selected || busy) return;
    const wording = selected.state === "stopped" ? "删除保留的源码" : "停止模块并删除源码";
    if (!window.confirm(`${wording} ${selected.name}？`)) return;
    setBusy(true);
    setError("");
    const result = await api.scriptRemove(selected.name);
    setBusy(false);
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setSelectedName("");
    setSource("");
    setSourceMeta(null);
    await refresh();
  }

  function closeEditor() {
    setEditorOpen(false);
    if (creating) {
      setCreating(false);
      setSource("");
      setDraftName("");
      if (selected) setSelectedName(selected.name);
    }
  }

  const stopped = selected?.state === "stopped";
  const sourceAvailable = Boolean(sourceMeta?.source_available);

  return (
    <section className="pba-panel pbm-panel">
      <header className="pba-panel-head">
        <div>
          <b>模块脚本</b>
          <span>运行状态 · 实时输出 · 类 Frida Script</span>
        </div>
        <button className="primary" onClick={beginCreate}>＋ 新建模块</button>
      </header>

      {error && <div className="pba-error" role="alert">{error}</div>}

      <div className="pbm-workspace">
        <aside className="pbm-sidebar">
          <div className="pbm-sidebar-head">
            <b>运行模块</b>
            <button onClick={refresh}>刷新</button>
          </div>
          <div className="pbm-module-list">
            {modules.length === 0 && (
              <div className="pba-empty"><b>暂无模块脚本</b><span>回调脚本不会出现在这里</span></div>
            )}
            {modules.map((script) => {
              const status = moduleStatus(script);
              return (
                <button
                  key={script.name}
                  className={`pbm-module-row ${script.name === selectedName ? "active" : ""}`}
                  onClick={() => setSelectedName(script.name)}
                >
                  <i className={status.tone} />
                  <span><b>{script.name}</b><small>{scriptKindLabel(script)} · generation {script.generation || "0"}</small></span>
                  <em>{status.label}</em>
                </button>
              );
            })}
          </div>
          <footer>{modules.length} 个模块 · callback 独立管理</footer>
        </aside>

        <div className="pbm-main">
          {!selected && (
            <div className="pbm-welcome">
              <i>PY</i>
              <b>创建一个连续分析模块</b>
              <span>模块可以先布置 Hook，在命中后检查现场，再动态启动 Trace 或下一阶段规则。</span>
              <button className="primary" onClick={beginCreate}>新建模块脚本</button>
            </div>
          )}

          {selected && (
            <>
              <section className="pbm-summary">
                <div className="pbm-title">
                  <i>PY</i>
                  <span><b>{selected.name}</b><small>{moduleStatus(selected).label} · {actorLabel(selected.modified_by)} 修改</small></span>
                </div>
                <div className="pbm-actions">
                  <button disabled={busy || !sourceAvailable} onClick={beginEdit}>开发脚本</button>
                  {stopped
                    ? <button className="primary" disabled={busy || selected.kind !== "module"} onClick={() => changeRuntime("start")}>启动</button>
                    : <button disabled={busy || selected.kind !== "module" || !sourceAvailable} onClick={() => changeRuntime("stop")}>停止</button>}
                  <button className="danger" disabled={busy} onClick={removeSelected}>删除</button>
                </div>
              </section>

              <section className="pbm-metrics">
                <Metric label="资源类型" value={scriptKindLabel(selected)} />
                <Metric label="源码代次" value={selected.generation || "0"} />
                <Metric label="已派发事件" value={selected.registration?.delivered || "0"} />
                <Metric label="丢弃事件" value={selected.registration?.dropped || "0"} warn={Number(selected.registration?.dropped || 0) > 0} />
              </section>

              <section className="pbm-output-card">
                <header>
                  <b>模块实时输出</b>
                  <span>
                    {loadingSource ? "读取模块信息…" : sourceAvailable ? `源码 generation ${sourceMeta?.generation || selected.generation || "0"} · 开发时弹出编辑器` : "当前 Hub 没有源码副本"}
                    {` · ${selectedOutput.length} 行`}
                  </span>
                </header>
                <div className="pbm-output">
                  {selectedOutput.length === 0 && <div className="pba-detail-empty">模块尚无输出；运行日志、分析结果和错误会集中显示在这里。</div>}
                  {selectedOutput.map((line) => <div key={`${line.seq}:${line.line}`}><em>#{line.seq}</em><span>{line.line}</span></div>)}
                </div>
              </section>
            </>
          )}
        </div>
      </div>

      <CallbackEditorDialog
        open={editorOpen}
        creating={creating}
        name={draftName || selected?.name || ""}
        source={source}
        meta={sourceMeta}
        error={error}
        loading={loadingSource}
        saving={busy}
        readOnly={!creating && !sourceAvailable}
        moduleMode
        onClose={closeEditor}
        onApply={saveSource}
      />
    </section>
  );
}

function Metric({ label, value, warn = false }) {
  return <div className={warn ? "warn" : ""}><span>{label}</span><b>{value}</b></div>;
}

function moduleStatus(script) {
  if (script.state === "stopped") return { label: "已停止", tone: "stopped" };
  const state = String(script.registration?.state || "");
  if (state === "1") return { label: "运行中", tone: "running" };
  if (state === "2") return { label: "错误", tone: "error" };
  if (state === "3") return { label: "替换中", tone: "staging" };
  if (state === "4") return { label: "初始化", tone: "staging" };
  if (script.state === "replacement_staged") return { label: "更新已提交", tone: "staging" };
  if (script.state === "load_staged") return { label: "加载已提交", tone: "staging" };
  return { label: "Agent 已加载", tone: "unknown" };
}

function scriptKindLabel(script) {
  if (script.kind === "module") return "模块脚本";
  if (script.kind === "callback") return "回调脚本";
  return "Agent 未分类脚本";
}

function actorLabel(actor) {
  if (actor === "human") return "人工";
  if (actor === "ai") return "AI/MCP";
  if (actor === "system") return "系统";
  return actor || "未知来源";
}
