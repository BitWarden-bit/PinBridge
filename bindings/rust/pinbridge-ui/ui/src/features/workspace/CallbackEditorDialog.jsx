import React, { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import Editor, { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor/editor/editor.api.js";
import "monaco-editor/languages/definitions/python/register.js";
import EditorWorker from "monaco-editor/editor/editor.worker.js?worker";
import { PB_API_CATALOG } from "./pbApiCatalog";

// Monaco is the editor core used by VS Code. Keep all assets local so the
// desktop UI does not depend on a CDN or an internet connection.
globalThis.MonacoEnvironment = {
  getWorker() {
    return new EditorWorker();
  },
};
loader.config({ monaco });

let pinbridgeLanguageRegistered = false;

function registerPinbridgeLanguage(monacoApi) {
  if (pinbridgeLanguageRegistered) return;
  pinbridgeLanguageRegistered = true;

  monacoApi.editor.defineTheme("pinbridge-python", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: "74808c", fontStyle: "italic" },
      { token: "keyword", foreground: "c6a8e7" },
      { token: "string", foreground: "9fc7a8" },
      { token: "number", foreground: "d5b875" },
      { token: "identifier", foreground: "d7d9dc" },
    ],
    colors: {
      "editor.background": "#0b0c0e",
      "editor.foreground": "#d7d9dc",
      "editorLineNumber.foreground": "#454a51",
      "editorLineNumber.activeForeground": "#aeb3b9",
      "editorCursor.foreground": "#d8d8d8",
      "editor.selectionBackground": "#39435688",
      "editor.inactiveSelectionBackground": "#2b334266",
      "editor.lineHighlightBackground": "#12151a",
      "editorIndentGuide.background1": "#202329",
      "editorIndentGuide.activeBackground1": "#454b54",
      "editorSuggestWidget.background": "#15171a",
      "editorSuggestWidget.border": "#363a41",
      "editorSuggestWidget.selectedBackground": "#292d34",
      "editorHoverWidget.background": "#15171a",
      "editorHoverWidget.border": "#363a41",
    },
  });

}

// Register the local theme immediately; the editor deliberately has no
// completion, hover or signature-help providers.
registerPinbridgeLanguage(monaco);

export default function CallbackEditorDialog({
  open,
  creating,
  name,
  source,
  meta,
  error,
  loading,
  saving,
  readOnly,
  callbackKind = "断点",
  moduleMode = false,
  onClose,
  onApply,
}) {
  const [draftName, setDraftName] = useState(name || "");
  const [draftSource, setDraftSource] = useState(source || "");
  const [query, setQuery] = useState("");
  const editorRef = useRef(null);
  const dirty = draftName !== (name || "") || draftSource !== (source || "");
  const resourceLabel = moduleMode ? "模块脚本" : `${callbackKind}回调`;

  useEffect(() => {
    if (!open) return;
    setDraftName(name || "");
    setDraftSource(source || "");
    setQuery("");
  }, [open, name, source]);

  useEffect(() => {
    if (!open) return undefined;
    function handleKeyDown(event) {
      if (event.key === "Escape") requestClose();
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (!saving && !loading && !readOnly && draftName.trim() && draftSource.trim()) {
          onApply?.({ name: draftName.trim(), source: draftSource });
        }
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, dirty, saving, loading, readOnly, draftName, draftSource]);

  const groups = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = needle
      ? PB_API_CATALOG.filter((item) => `${item.name} ${item.group} ${item.documentation} ${item.signature}`.toLowerCase().includes(needle))
      : PB_API_CATALOG;
    return filtered.reduce((result, item) => {
      if (!result.has(item.group)) result.set(item.group, []);
      result.get(item.group).push(item);
      return result;
    }, new Map());
  }, [query]);

  function requestClose() {
    if (saving) return;
    if (dirty && !window.confirm("代码尚未保存，确定关闭编辑器吗？")) return;
    onClose?.();
  }

  function insertApi(item) {
    const editor = editorRef.current;
    if (!editor || readOnly) return;
    editor.focus();
    const selection = editor.getSelection();
    editor.executeEdits("pinbridge-api-reference", [{ range: selection, text: `pb.${stripSnippetMarkers(item.snippet)}`, forceMoveMarkers: true }]);
    editor.setPosition(editor.getModel().getPositionAt(editor.getModel().getOffsetAt(selection.getStartPosition()) + 3 + stripSnippetMarkers(item.snippet).length));
  }

  if (!open) return null;
  const lines = draftSource ? draftSource.split(/\r?\n/).length : 0;
  return createPortal(
    <div className="pba-editor-backdrop" role="presentation">
      <section className="pba-editor-dialog" role="dialog" aria-modal="true" aria-label={creating ? `新建${resourceLabel}` : `编辑${resourceLabel}`}>
        <header className="pba-editor-head">
          <div className="pba-editor-mark">PY</div>
          <div className="pba-editor-heading">
            <b>{creating ? `新建${resourceLabel}` : `${resourceLabel}代码编辑器`}</b>
            <span>Monaco / Python · PinBridge {resourceLabel}</span>
          </div>
          <div className="pba-editor-stats"><span>{lines} 行</span><span>{PB_API_CATALOG.length} 个 pb API</span>{dirty && <i>未保存</i>}</div>
          <button className="pba-editor-close" onClick={requestClose} aria-label="关闭代码编辑器">×</button>
        </header>

        <div className="pba-editor-filebar">
          <span>脚本</span>
          <input value={draftName} readOnly={!creating} onChange={(event) => setDraftName(event.target.value)} spellCheck="false" aria-label="脚本名称" />
          <code>Python 3</code>
          {meta && <em>generation {meta.generation ?? "—"}</em>}
        </div>

        {error && <div className="pba-editor-error" role="alert">{error}</div>}
        <div className="pba-editor-body">
          <div className="pba-monaco-host">
            {loading && <div className="pba-editor-loading">读取脚本…</div>}
            <Editor
              value={draftSource}
              language="python"
              theme="pinbridge-python"
              beforeMount={registerPinbridgeLanguage}
              onMount={(editor) => {
                editorRef.current = editor;
                editor.focus();
              }}
              onChange={(value) => setDraftSource(value || "")}
              loading={<div className="pba-editor-loading">正在加载 Monaco 编辑器…</div>}
              options={{
                readOnly: loading || readOnly,
                automaticLayout: true,
                fontFamily: '"Cascadia Code", "JetBrains Mono", Consolas, monospace',
                fontSize: 13,
                lineHeight: 21,
                tabSize: 4,
                insertSpaces: true,
                minimap: { enabled: true, maxColumn: 90, scale: 1 },
                folding: true,
                glyphMargin: true,
                bracketPairColorization: { enabled: true },
                guides: { bracketPairs: true, indentation: true },
                quickSuggestions: false,
                suggestOnTriggerCharacters: false,
                parameterHints: { enabled: false },
                wordWrap: "off",
                scrollBeyondLastLine: false,
                smoothScrolling: true,
                padding: { top: 12, bottom: 18 },
              }}
            />
          </div>

          <aside className="pba-api-reference">
            <div className="pba-api-reference-head">
              <b>PinBridge API</b>
              <span>点击条目插入调用</span>
              <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索全部 API…" spellCheck="false" />
            </div>
            <div className="pba-api-list">
              {[...groups.entries()].map(([group, items]) => (
                <section key={group}>
                  <h3>{group}<span>{items.length}</span></h3>
                  {items.map((item) => (
                    <button key={item.name} onClick={() => insertApi(item)} title="点击插入调用" disabled={readOnly}>
                      <code>pb.{item.name}</code>
                      <span>{item.documentation}</span>
                      <em>{item.signature}</em>
                    </button>
                  ))}
                </section>
              ))}
              {groups.size === 0 && <div className="pba-api-empty">无匹配 API</div>}
            </div>
          </aside>
        </div>

        <footer className="pba-editor-foot">
          <span><kbd>Ctrl</kbd> + <kbd>S</kbd> 保存 · 右侧 API 列表可点击插入</span>
          <button onClick={requestClose}>取消</button>
          <button
            className="primary"
            disabled={saving || loading || readOnly || !draftName.trim() || !draftSource.trim()}
            onClick={() => onApply?.({ name: draftName.trim(), source: draftSource })}
          >{saving ? "应用中…" : creating ? (moduleMode ? "保存并运行模块" : "应用并加载回调") : "应用新版本"}</button>
        </footer>
      </section>
    </div>,
    document.body,
  );
}

function stripSnippetMarkers(snippet) {
  return snippet.replace(/\$\{\d+:([^}]*)\}/g, "$1").replace(/\$\d+/g, "");
}
