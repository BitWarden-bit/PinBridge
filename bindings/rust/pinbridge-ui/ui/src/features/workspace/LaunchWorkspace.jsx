import React, { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, callWithError } from "../../api";

function storedRecents() {
  try {
    const value = JSON.parse(localStorage.getItem("pb-recent-sessions") || "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}

function splitArguments(text) {
  const values = [];
  let value = "";
  let quote = null;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if ((char === '"' || char === "'") && (!quote || quote === char)) {
      quote = quote ? null : char;
    } else if (/\s/.test(char) && !quote) {
      if (value) values.push(value);
      value = "";
    } else if (char === "\\" && quote && text[index + 1] === quote) {
      value += quote;
      index += 1;
    } else {
      value += char;
    }
  }
  if (value) values.push(value);
  return values;
}

function fileName(path) {
  return path.split(/[\\/]/).filter(Boolean).pop() || path;
}

export default function LaunchWorkspace({ onLaunch, releasedSession }) {
  const [target, setTarget] = useState(() => localStorage.getItem("pb-target") || "");
  const [args, setArgs] = useState(() => localStorage.getItem("pb-target-args") || "");
  const [pin, setPin] = useState(() => localStorage.getItem("pb-pin") || "");
  const [execMode, setExecMode] = useState(() => localStorage.getItem("pb-probe-mode") === "1" ? "probe" : "jit");
  const [entryBp, setEntryBp] = useState(() => localStorage.getItem("pb-entry-bp") !== "0");
  const [attachAddr, setAttachAddr] = useState(() => localStorage.getItem("pb-agent-address") || "127.0.0.1:9011");
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [recents, setRecents] = useState(storedRecents);
  const [environment, setEnvironment] = useState(null);

  const probe = execMode === "probe";
  const envChecks = useMemo(() => [
    { name: "Intel Pin 3.31", note: environment?.pin?.path || "未检测到；可手动选择 pin.exe", tone: environment?.pin?.available ? "ok" : "idle" },
    { name: "Agent x64", note: environment?.agent?.path || "未找到 Release Agent", tone: environment?.agent?.available ? "ok" : "idle" },
    { name: "Python runtime", note: environment?.python?.detail || "由 Agent 在启动时校验", tone: environment?.python?.available ? "ok" : "idle" },
    { name: "Hub / MCP", note: environment?.hub?.detail || "正在检查", tone: environment?.hub?.available ? "ok" : "idle" },
  ], [environment]);

  useEffect(() => {
    let live = true;
    api.environment().then((result) => {
      if (live && result.ok) setEnvironment(result.value);
    });
    return () => { live = false; };
  }, []);

  async function browseTarget() {
    const picked = await open({ title: "选择目标程序", filters: [{ name: "Windows executable", extensions: ["exe"] }] });
    if (picked) {
      setTarget(picked);
      localStorage.setItem("pb-target", picked);
    }
  }

  async function browsePin() {
    const picked = await open({ title: "选择 Intel Pin", filters: [{ name: "pin.exe", extensions: ["exe"] }] });
    if (picked) {
      setPin(picked);
      localStorage.setItem("pb-pin", picked);
    }
  }

  function rememberSession(path, mode) {
    const next = [{
      name: fileName(path), path, mode, time: "刚刚", result: "会话已启动", tone: "ok",
    }, ...recents.filter((item) => item.path.toLowerCase() !== path.toLowerCase())].slice(0, 6);
    setRecents(next);
    localStorage.setItem("pb-recent-sessions", JSON.stringify(next));
  }

  async function launch(preset) {
    const launchTarget = preset || target.trim();
    if (!launchTarget || busy) return;
    setBusy("launch");
    setError("");
    localStorage.setItem("pb-target", launchTarget);
    localStorage.setItem("pb-target-args", args);
    localStorage.setItem("pb-entry-bp", entryBp ? "1" : "0");
    localStorage.setItem("pb-probe-mode", probe ? "1" : "0");
    if (pin) localStorage.setItem("pb-pin", pin);
    const result = await callWithError("cmd_launch", {
      target: launchTarget,
      pin: pin.trim() || null,
      entryBp: probe ? false : entryBp,
      probeMode: probe,
      arguments: splitArguments(args),
    });
    setBusy("");
    if (!result.ok) {
      setError(result.error);
      return;
    }
    rememberSession(launchTarget, probe ? "观察" : "JIT");
    onLaunch?.({ target: launchTarget, mode: probe ? "probe" : "jit", attached: false });
  }

  async function attach(preferredAddress = "") {
    const address = String(preferredAddress || attachAddr).trim();
    if (!address || busy) return;
    setBusy("attach");
    setError("");
    localStorage.setItem("pb-agent-address", address);
    setAttachAddr(address);
    const result = await api.attachAgent(address);
    setBusy("");
    if (!result.ok) {
      setError(result.error);
      return;
    }
    const session = result.value?.session || result.value || {};
    onLaunch?.({ target: session.target || `Agent ${address}`, mode: "attach", attached: true });
  }

  return (
    <div className="pbw-launch">
      <header className="pbw-launch-brand">
        <span className="pbw-brandmark">PB</span>
        <div><b>PinBridge</b><span>可编程动态二进制分析工作台 · Windows x64 / x86</span></div>
        <em>ABI v1.11 · Intel Pin 3.31</em>
      </header>

      <main className="pbw-launch-grid">
        <section className="pbw-launch-card">
          <div className="pbw-launch-title"><b>启动新目标</b><span>pin.exe + pinbridge_agent.dll</span></div>
          <label className="pbw-launch-field">
            <span>目标程序</span>
            <div><input value={target} placeholder="C:\\path\\to\\target.exe" onChange={(event) => setTarget(event.target.value)} onKeyDown={(event) => event.key === "Enter" && launch()} spellCheck="false" /><button onClick={browseTarget}>浏览</button></div>
          </label>
          <label className="pbw-launch-field">
            <span>命令行参数</span>
            <div><input value={args} placeholder="可选；支持带引号的参数" onChange={(event) => setArgs(event.target.value)} spellCheck="false" /></div>
          </label>
          <label className="pbw-launch-field">
            <span>Intel Pin</span>
            <div><input value={pin} placeholder={environment?.pin?.path || "留空自动检测"} onChange={(event) => setPin(event.target.value)} spellCheck="false" /><button onClick={browsePin}>浏览</button></div>
          </label>
          <div className="pbw-launch-field">
            <span>执行模式</span>
            <div className="pbw-mode-cards">
              <button className={execMode === "jit" ? "active" : ""} onClick={() => setExecMode("jit")}><b>JIT 调试<i>推荐</i></b><span>断点 · 单步 · 异常接管 · 系统调用 · 插桩 Trace</span></button>
              <button className={execMode === "probe" ? "active" : ""} onClick={() => setExecMode("probe")}><b>原生兼容观察<i>保底</i></b><span>机器码原生执行，仅保留模块 / 生命周期观察，调试能力受限</span></button>
            </div>
          </div>
          <label className={`pbw-launch-check ${probe ? "disabled" : ""}`}><input type="checkbox" checked={entryBp && !probe} disabled={probe} onChange={(event) => setEntryBp(event.target.checked)} /><span>在程序入口设置断点{probe && " · 观察模式下不可用"}</span></label>
          {error && <div className="pbw-launch-error" role="alert">{error}</div>}
          <div className="pbw-launch-actions"><button className="primary" disabled={!!busy || !target.trim()} onClick={() => launch()}>{busy === "launch" ? "正在启动…" : "启动分析"}</button><span>{busy === "launch" ? "Pin + Agent" : ""}</span></div>
        </section>

        <aside className="pbw-launch-side">
          <section className="pbw-launch-card">
            <div className="pbw-launch-title"><b>最近会话</b><span>{recents.length} 条</span></div>
            {recents.length === 0 && <div className="pbw-recent-empty">无最近会话</div>}
            {recents.map((item) => <button key={item.path} className="pbw-recent" disabled={!!busy} onClick={() => launch(item.path)}><i className={item.tone} /><div><b>{item.name}</b><span>{item.path}</span></div><em>{item.mode}<small>{item.time} · {item.result}</small></em></button>)}
          </section>
          <section className="pbw-launch-card">
            <div className="pbw-launch-title"><b>附加到运行中 Agent</b><span>仅允许 loopback</span></div>
            {releasedSession && <button className="pbw-reattach-card" disabled={!!busy} onClick={() => attach(releasedSession.address)}>
              <i />
              <span><b>重新附加刚释放的会话</b><small>{releasedSession.target || "当前目标"} · {releasedSession.address}</small></span>
              <em>{busy === "attach" ? "连接中…" : "重新附加"}</em>
            </button>}
            <div className="pbw-launch-attach"><input value={attachAddr} onChange={(event) => setAttachAddr(event.target.value)} onKeyDown={(event) => event.key === "Enter" && attach()} spellCheck="false" /><button disabled={!!busy} onClick={() => attach()}>{busy === "attach" ? "连接中…" : "连接"}</button></div>
            <p>释放和重新附加只切换工作区控制，不重启、不终止目标。</p>
          </section>
          <section className="pbw-launch-card">
            <div className="pbw-launch-title"><b>环境自检</b><span>来自本机运行时</span></div>
            {envChecks.map((check) => <div key={check.name} className="pbw-env-row"><i className={check.tone} /><b>{check.name}</b><span title={check.note}>{check.note}</span></div>)}
          </section>
        </aside>
      </main>
      <footer className="pbw-launch-foot"><span>PinBridge Runtime</span><span>Human · AI</span></footer>
    </div>
  );
}
