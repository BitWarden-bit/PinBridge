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
    memoryMap: "Memory Map",
    events: "Events",
    read: "Read",
    write: "Write",
    delete: "Delete",
    hits: "Hits", memoryEditHint: "Click bytes to edit; press Enter to write", clickBreakpoint: "Click to set breakpoint",
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
    flagToggleHint: "click to toggle",
    platform: "Dynamic Analysis Platform",
    targetPlaceholder: "Target executable path…",
    pinPlaceholder: "pin.exe path (empty = PIN_EXE / PIN_ROOT env)",
    browse: "Browse…",
    launch: "Launch Analysis",
    launching: "Launching…",
    launchFailed: "Launch failed — check the pin.exe path",
    launchHint: "Agent DLL auto-resolve · session cleanup on close",
    entryBp: "Break at program entry",
    probeMode: "Native compatibility observation (debugging limited)",
    probeModeHint: "Modules / lifecycle · no breakpoints, stepping, exceptions, syscalls, or Trace",
    breakpointAt: "Breakpoint @ ",
    controlMode: "Control mode", manualMode: "Manual", aiLedMode: "AI-led", controlOwner: "Control",
    aiControl: "AI", aiPaused: "AI paused", manualControl: "Manual", unknownState: "Unknown", permission: "Permission",
    permissionReadOnly: "Read-only", permissionAssist: "Assist", permissionAutonomous: "Autonomous", permissionPaused: "Paused", permissionManual: "—",
    handoffToAi: "Hand off to AI", takeoverNow: "Take over now", handingOff: "Handing off…", takingOver: "Taking over…",
    handoffFailed: "Handoff failed", takeoverFailed: "Takeover failed", handoffComplete: "AI control active", takeoverComplete: "Manual control active",
    aiDebugDesk: "AI DEBUG DESK", aiDeskSubtitle: "Activity · control · handoff",
    controlServiceConnected: "Control service connected", checkingService: "Checking control service…", controlServiceOffline: "Control service unavailable", readOnlyFallback: "Showing safe empty state",
    sessionStatus: "Session status", session: "Session", targetPid: "Target PID", targetState: "Target state", targetStateUnknown: "Unknown", stopAddress: "Stop address", stopThread: "Thread", stopReason: "Stop reason", currentOperation: "Current operation", currentScript: "Current script",
    activityTimeline: "Activity timeline", activityStructuredHint: "Time order", filterActivity: "Filter activity", allActivity: "All activity",
    actor: "Actor", purpose: "Purpose", outcome: "Outcome", resource: "Resource", parent: "Parent", type: "Type", operationId: "Operation ID", startedAt: "Started (ms)", completedAt: "Completed (ms)", before: "Before", after: "After", purposeUnavailable: "Purpose not provided",
    assetOverview: "Assets", hooks: "Hooks", dynamicScripts: "Dynamic scripts", collectionTasks: "Collection tasks", notAvailable: "Not available",
    activityDetails: "Activity details", selectActivity: "No activity selected", activityServiceUnavailable: "Activity service offline", activityServiceHint: "Waiting for control service", noActivityYet: "No activity records", activityEmptyHint: "No records",
    aiNeedsSession: "No active session", aiNeedsSessionHint: "Launch or attach a target", returnToManual: "Return to Manual",
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
    memoryMap: "内存布局",
    events: "事件",
    read: "读取",
    write: "写入",
    delete: "删除",
    hits: "命中", memoryEditHint: "点击字节直接编辑；按回车写入", clickBreakpoint: "点击下断点",
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
    flagToggleHint: "点击取反写回",
    platform: "动态执行分析平台",
    targetPlaceholder: "目标可执行文件路径…",
    pinPlaceholder: "pin.exe 路径(留空走 PIN_EXE / PIN_ROOT)",
    browse: "浏览…",
    launch: "启动分析",
    launching: "正在启动…",
    launchFailed: "启动失败——检查 pin.exe 路径",
    launchHint: "Agent DLL 自动解析 · 关闭窗口时回收会话",
    entryBp: "断在程序入口",
    probeMode: "原生兼容观察模式（调试能力受限）",
    probeModeHint: "模块 / 生命周期 · 无断点、单步、异常、系统调用和 Trace",
    breakpointAt: "断点 @ ",
    controlMode: "控制模式", manualMode: "古法", aiLedMode: "AI 主导", controlOwner: "当前控制权",
    aiControl: "AI", aiPaused: "AI 已暂停", manualControl: "人", unknownState: "未知", permission: "权限", permissionReadOnly: "只读", permissionAssist: "协助", permissionAutonomous: "自主", permissionPaused: "已暂停", permissionManual: "—",
    handoffToAi: "交给 AI", takeoverNow: "立即接管", handingOff: "交接中…", takingOver: "接管中…", handoffFailed: "交接失败", takeoverFailed: "接管失败", handoffComplete: "AI 已获得控制权", takeoverComplete: "已恢复人工控制",
    aiDebugDesk: "AI 调试活动台", aiDeskSubtitle: "活动 · 控制权 · 交接", controlServiceConnected: "控制服务已连接", checkingService: "检查控制服务…", controlServiceOffline: "控制服务未连接", readOnlyFallback: "只读",
    sessionStatus: "会话状态", session: "会话", targetPid: "目标 PID", targetState: "目标状态", targetStateUnknown: "未知", stopAddress: "停止地址", stopThread: "线程", stopReason: "停止原因", currentOperation: "当前操作", currentScript: "当前脚本",
    activityTimeline: "活动时间线", activityStructuredHint: "按时间排序", filterActivity: "筛选活动", allActivity: "全部活动", actor: "执行者", purpose: "目的", outcome: "结果", resource: "资源", parent: "关联 parent", type: "类型", operationId: "操作 ID", startedAt: "开始 (ms)", completedAt: "完成 (ms)", before: "修改前", after: "修改后", purposeUnavailable: "未提供目的",
    assetOverview: "资产概览", hooks: "Hook", dynamicScripts: "动态脚本", collectionTasks: "采集任务", notAvailable: "无数据", activityDetails: "活动详情", selectActivity: "未选择活动", activityServiceUnavailable: "活动服务未连接", activityServiceHint: "等待控制服务", noActivityYet: "无活动记录", activityEmptyHint: "无记录",
    aiNeedsSession: "无活动会话", aiNeedsSessionHint: "先启动或附加目标", returnToManual: "返回人工模式",
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
