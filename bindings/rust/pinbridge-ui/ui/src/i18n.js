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
    platform: "Dynamic Analysis Platform",
    targetPlaceholder: "Target executable path…",
    pinPlaceholder: "pin.exe path (empty = PIN_EXE / PIN_ROOT env)",
    browse: "Browse…",
    launch: "Launch Analysis",
    launching: "Launching…",
    launchFailed: "Launch failed — check the pin.exe path",
    launchHint: "Agent DLL is auto-resolved next to this app. Session is reaped when the window closes.",
    entryBp: "Break at program entry",
    probeMode: "VMP / exception-compatible probe mode",
    probeModeHint: "Runs application code natively. Entry breakpoints, stepping, syscall and instruction-level instrumentation are unavailable.",
    breakpointAt: "Breakpoint @ ",
    controlMode: "Control mode", manualMode: "Manual", aiLedMode: "AI-led", controlOwner: "Control",
    aiControl: "AI", aiPaused: "AI paused", manualControl: "Manual", unknownState: "Unknown", permission: "Permission",
    permissionReadOnly: "Read-only", permissionAssist: "Assist", permissionAutonomous: "Autonomous", permissionPaused: "Paused", permissionManual: "—",
    handoffToAi: "Hand off to AI", takeoverNow: "Take over now", handingOff: "Handing off…", takingOver: "Taking over…",
    handoffFailed: "Handoff failed", takeoverFailed: "Takeover failed", handoffComplete: "AI control active", takeoverComplete: "Manual control active",
    aiDebugDesk: "AI DEBUG DESK", aiDeskSubtitle: "Structured activity, explicit control, and a reversible handoff.",
    controlServiceConnected: "Control service connected", checkingService: "Checking control service…", controlServiceOffline: "Control service unavailable", readOnlyFallback: "Showing safe empty state",
    sessionStatus: "Session status", session: "Session", targetPid: "Target PID", targetState: "Target state", targetStateUnknown: "Unknown", stopAddress: "Stop address", stopThread: "Thread", stopReason: "Stop reason", currentOperation: "Current operation", currentScript: "Current script",
    activityTimeline: "Activity timeline", activityStructuredHint: "Events are structured records, not a text log.", filterActivity: "Filter activity", allActivity: "All activity",
    actor: "Actor", purpose: "Purpose", outcome: "Outcome", resource: "Resource", parent: "Parent", type: "Type", operationId: "Operation ID", startedAt: "Started (ms)", completedAt: "Completed (ms)", before: "Before", after: "After", purposeUnavailable: "Purpose not provided",
    assetOverview: "Assets", hooks: "Hooks", dynamicScripts: "Dynamic scripts", collectionTasks: "Collection tasks", notAvailable: "Not available",
    activityDetails: "Activity details", selectActivity: "Select an activity to inspect its structured fields.", activityServiceUnavailable: "Activity service is not connected", activityServiceHint: "Connect the control service to load real activity records.", noActivityYet: "No activity records", activityEmptyHint: "No records were returned for this session.",
    aiNeedsSession: "AI mode needs an active session", aiNeedsSessionHint: "Launch or attach from Manual mode first. The current target is never relaunched by AI mode.", returnToManual: "Return to Manual",
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
    platform: "动态执行分析平台",
    targetPlaceholder: "目标可执行文件路径…",
    pinPlaceholder: "pin.exe 路径(留空走 PIN_EXE / PIN_ROOT)",
    browse: "浏览…",
    launch: "启动分析",
    launching: "正在启动…",
    launchFailed: "启动失败——检查 pin.exe 路径",
    launchHint: "agent DLL 自动从本程序旁解析;窗口关闭时会话自动回收。",
    entryBp: "断在程序入口",
    probeMode: "VMP / 异常兼容探针模式",
    probeModeHint: "目标机器码原生运行；入口断点、单步、系统调用和指令级插桩不可用。",
    breakpointAt: "断点 @ ",
    controlMode: "控制模式", manualMode: "古法", aiLedMode: "AI 主导", controlOwner: "当前控制权",
    aiControl: "AI", aiPaused: "AI 已暂停", manualControl: "人", unknownState: "未知", permission: "权限", permissionReadOnly: "只读", permissionAssist: "协助", permissionAutonomous: "自主", permissionPaused: "已暂停", permissionManual: "—",
    handoffToAi: "交给 AI", takeoverNow: "立即接管", handingOff: "交接中…", takingOver: "接管中…", handoffFailed: "交接失败", takeoverFailed: "接管失败", handoffComplete: "AI 已获得控制权", takeoverComplete: "已恢复人工控制",
    aiDebugDesk: "AI 调试活动台", aiDeskSubtitle: "结构化活动、明确控制权、可逆交接。", controlServiceConnected: "控制服务已连接", checkingService: "正在检查控制服务…", controlServiceOffline: "控制服务未连接", readOnlyFallback: "当前为安全空状态",
    sessionStatus: "会话状态", session: "会话", targetPid: "目标 PID", targetState: "目标状态", targetStateUnknown: "未知", stopAddress: "停止地址", stopThread: "线程", stopReason: "停止原因", currentOperation: "当前操作", currentScript: "当前脚本",
    activityTimeline: "活动时间线", activityStructuredHint: "结构化记录，不是纯文本日志。", filterActivity: "筛选活动", allActivity: "全部活动", actor: "执行者", purpose: "目的", outcome: "结果", resource: "资源", parent: "关联 parent", type: "类型", operationId: "操作 ID", startedAt: "开始 (ms)", completedAt: "完成 (ms)", before: "修改前", after: "修改后", purposeUnavailable: "未提供目的",
    assetOverview: "资产概览", hooks: "Hook", dynamicScripts: "动态脚本", collectionTasks: "采集任务", notAvailable: "暂无数据", activityDetails: "活动详情", selectActivity: "选择活动以查看结构化字段。", activityServiceUnavailable: "活动服务未连接", activityServiceHint: "连接控制服务后加载真实活动记录。", noActivityYet: "暂无活动记录", activityEmptyHint: "此会话没有返回活动记录。",
    aiNeedsSession: "AI 模式需要活动会话", aiNeedsSessionHint: "请先在古法模式启动或附加目标；AI 模式不会强制重新启动目标。", returnToManual: "返回古法模式",
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
