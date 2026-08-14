import { useSyncExternalStore } from "react";

const dicts = {
  en: {
    continue: "Continue",
    pause: "Pause",
    stepInto: "Step Into",
    stepOver: "Step Over",
    followRip: "Follow RIP",
    go: "Go",
    target: "Target",
    addressPlaceholder: "0x address",
    disassembly: "Disassembly",
    registers: "Registers",
    pauseToSelect: "(pause to select)",
    eventRate: "Event Rate",
    memory: "Memory",
    stack: "Stack",
    breakpoints: "Breakpoints",
    modules: "Modules",
    events: "Events",
    read: "Read",
    write: "Write",
    delete: "Delete",
    hits: "Hits",
    base: "Base",
    end: "End",
    name: "Name",
    connected: "Connected",
    disconnected: "Disconnected",
    running: "Running",
    stopped: "Stopped",
    noTarget: "No target loaded",
    pauseFirst: "Pause first",
    newValue: "New value",
    platform: "Dynamic Analysis Platform",
    targetPlaceholder: "Target executable path…",
    pinPlaceholder: "pin.exe path (empty = PIN_EXE / PIN_ROOT env)",
    browse: "Browse…",
    launch: "Launch Analysis",
    launching: "Launching…",
    launchFailed: "Launch failed — check the pin.exe path",
    launchHint: "Agent DLL is auto-resolved next to this app. Session is reaped when the window closes.",
    entryBp: "Break at program entry",
    breakpointAt: "Breakpoint @ ",
  },
  zh: {
    continue: "继续",
    pause: "暂停",
    stepInto: "步入",
    stepOver: "步过",
    followRip: "跟随 RIP",
    go: "转到",
    target: "目标",
    addressPlaceholder: "0x 地址",
    disassembly: "反汇编",
    registers: "寄存器",
    pauseToSelect: "(暂停后选择)",
    eventRate: "事件速率",
    memory: "内存",
    stack: "栈",
    breakpoints: "断点",
    modules: "模块",
    events: "事件",
    read: "读取",
    write: "写入",
    delete: "删除",
    hits: "命中",
    base: "基址",
    end: "结束",
    name: "名称",
    connected: "已连接",
    disconnected: "已断开",
    running: "运行中",
    stopped: "已断下",
    noTarget: "未加载目标",
    pauseFirst: "先暂停",
    newValue: "新值",
    platform: "动态执行分析平台",
    targetPlaceholder: "目标可执行文件路径…",
    pinPlaceholder: "pin.exe 路径(留空走 PIN_EXE / PIN_ROOT)",
    browse: "浏览…",
    launch: "启动分析",
    launching: "正在启动…",
    launchFailed: "启动失败——检查 pin.exe 路径",
    launchHint: "agent DLL 自动从本程序旁解析;窗口关闭时会话自动回收。",
    entryBp: "断在程序入口",
    breakpointAt: "断点 @ ",
  },
};

// Default UI language is Chinese; debugger terms that live in English in
// Chinese tech context (hook, exec, branch, sys, ctx, tid, rip, hex…) stay
// in English on purpose. The toolbar toggle switches to English and back;
// the choice persists via localStorage.
let lang = localStorage.getItem("pb-lang") || "zh";
const listeners = new Set();

export function getLang() {
  return lang;
}

export function setLang(next) {
  lang = next;
  localStorage.setItem("pb-lang", next);
  listeners.forEach((l) => l());
}

export function subscribeLang(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function t(key) {
  return dicts[lang][key] ?? key;
}

export function useT() {
  useSyncExternalStore(subscribeLang, getLang);
  return t;
}
