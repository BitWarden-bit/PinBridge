import { invoke } from "@tauri-apps/api/core";

// All backend calls funnel through here; failures set a visible status.
export async function call(fn, args = {}) {
  try {
    return await invoke(fn, args);
  } catch (e) {
    window.dispatchEvent(new CustomEvent("pb-error", { detail: `${fn}: ${e}` }));
    return undefined;
  }
}

// Variant that returns the error text to the caller (launch screen etc.).
export async function callWithError(fn, args = {}) {
  try {
    return { ok: true, value: await invoke(fn, args) };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

export const api = {
  control: (action) => call("cmd_control", { action }),
  launch: (target, options = {}) => call("cmd_launch", { target, ...options }),
  attachAgent: (address) => callWithError("cmd_attach_agent", { address }),
  environment: () => callWithError("cmd_environment"),
  killBackend: () => call("cmd_kill_backend"),
  releaseSession: () => callWithError("cmd_release_session"),
  session: () => call("cmd_session"),
  step: (tid, over) => call("cmd_step", { tid, over }),
  threads: () => call("cmd_threads"),
  context: (tid) => call("cmd_context", { tid }),
  setreg: (tid, reg, value) => call("cmd_setreg", { tid, reg, value }),
  disasm: (address, count = 64) => call("cmd_disasm", { address, count }),
  disasmUp: (address, count = 64) => call("cmd_disasm_up", { address, count }),
  bpSet: (address) => call("cmd_bp_set", { address }),
  bpRemove: (id) => call("cmd_bp_remove", { id: String(id) }),
  bpSetResult: (address) => callWithError("cmd_bp_set", { address }),
  bpRemoveResult: (id) => callWithError("cmd_bp_remove", { id: String(id) }),
  bpList: () => call("cmd_bp_list"),
  breakpointInventory: () => callWithError("cmd_breakpoint_inventory"),
  scriptGet: (name) => callWithError("cmd_script_get", { name }),
  scriptList: () => callWithError("cmd_script_list"),
  scriptInject: (name, source, kind = "module") => callWithError("cmd_script_inject", { name, source, kind }),
  scriptReplace: (name, source, kind) => callWithError("cmd_script_replace", { name, source, kind: kind || null }),
  scriptStart: (name) => callWithError("cmd_script_start", { name }),
  scriptStop: (name) => callWithError("cmd_script_stop", { name }),
  scriptRemove: (name) => callWithError("cmd_script_remove", { name }),
  scriptOutput: (cursor = "0", limit = "256") => callWithError("cmd_script_output", { cursor: String(cursor), limit: String(limit) }),
  modules: () => call("cmd_modules"),
  moduleExports: (module) => callWithError("cmd_module_exports", { module }),
  hookSet: (address) => callWithError("cmd_hook_set", { address }),
  hookFunctionSet: (address, signature, signatureSource, signatureConfidence) => callWithError("cmd_hook_function_set", { address, signature, signatureSource, signatureConfidence }),
  hookSignatureSet: (address, signature, signatureSource, signatureConfidence) => callWithError("cmd_hook_signature_set", { address, signature, signatureSource, signatureConfidence }),
  hookSignatureRemove: (address) => callWithError("cmd_hook_signature_remove", { address }),
  hookRemove: (address) => callWithError("cmd_hook_remove", { address }),
  hookClear: () => callWithError("cmd_hook_clear"),
  hookList: () => callWithError("cmd_hook_list"),
  hookInventory: (offset = 0, limit = 300, kind = "all") => callWithError("cmd_hook_inventory", { offset, limit, kind }),
  hookMonitor: (limit = 1024, before = "0") => callWithError("cmd_hook_monitor", { limit, before }),
  hookEventsQuery: (query = {}) => callWithError("cmd_hook_events_query", { query }),
  hookEventsExport: (query = {}) => callWithError("cmd_hook_events_export", { query }),
  traceScopeQuery: (query = {}) => callWithError("cmd_trace_scope_query", { query }),
  traceRecordStart: (query = {}) => callWithError("cmd_trace_record_start", { query }),
  traceRecordStatus: () => callWithError("cmd_trace_record_status"),
  traceRecordStop: () => callWithError("cmd_trace_record_stop"),
  traceIndexQuery: (query = {}) => callWithError("cmd_trace_index_query", { query }),
  traceIndexExport: (query = {}) => callWithError("cmd_trace_index_export", { query }),
  hookModule: (module) => callWithError("cmd_hook_module", { module }),
  hookRangePreview: (start, end, kinds) => callWithError("cmd_hook_range_preview", { start, end, kinds }),
  hookRangeSet: (start, end, kinds) => callWithError("cmd_hook_range_set", { start, end, kinds }),
  syscallConfigGet: () => callWithError("cmd_syscall_config_get"),
  syscallConfigSet: (enabled, numbers = [], scope = "all", module = "", rvaBegin = "0x0", rvaEnd = "0x0") => callWithError("cmd_syscall_config_set", { enabled, numbers, scope, module, rvaBegin, rvaEnd }),
  syscallMonitor: (limit = 256) => callWithError("cmd_syscall_monitor", { limit }),
  memoryMap: () => callWithError("cmd_memory_map"),
  exceptionMonitor: (limit = 256) => callWithError("cmd_exception_monitor", { limit }),
  exceptionPolicyGet: () => callWithError("cmd_exception_policy_get"),
  exceptionPolicySet: (enabled, code = "0") => callWithError("cmd_exception_policy_set", { enabled, code }),
  exceptionInventory: () => callWithError("cmd_exception_inventory"),
  readMem: (address, size = 256) => call("cmd_read_mem", { address, size }),
  writeMem: (address, data) => call("cmd_write_mem", { address, data }),
  resolve: (addresses) => call("cmd_resolve", { addresses }),
  // AI control is a thin adapter to the trusted Tauri Human adapter; an
  // unavailable Hub is surfaced as a connection error, never simulated.
  // Handoff authorization is injected by trusted Tauri code. React does not
  // receive, store, or forward an operator token.
  ai: {
    controlStatus: () => callWithError("control_status"),
    handoffToAi: () => callWithError("control_handoff_to_ai"),
    takeoverManual: () => callWithError("control_takeover_manual"),
    sessionStatus: () => callWithError("session_status"),
    activityList: (args = {}) => callWithError("activity_list", { ...args, limit: String(args.limit ?? "100") }),
    activityGet: (operationId) => callWithError("activity_get", { operation_id: operationId }),
  },
};
