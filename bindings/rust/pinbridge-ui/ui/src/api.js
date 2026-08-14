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
  launch: (target) => call("cmd_launch", { target }),
  killBackend: () => call("cmd_kill_backend"),
  session: () => call("cmd_session"),
  step: (tid, over) => call("cmd_step", { tid, over }),
  threads: () => call("cmd_threads"),
  context: (tid) => call("cmd_context", { tid }),
  setreg: (tid, reg, value) => call("cmd_setreg", { tid, reg, value }),
  disasm: (address, count = 64) => call("cmd_disasm", { address, count }),
  disasmUp: (address, count = 64) => call("cmd_disasm_up", { address, count }),
  bpSet: (address) => call("cmd_bp_set", { address }),
  bpRemove: (id) => call("cmd_bp_remove", { id }),
  bpList: () => call("cmd_bp_list"),
  modules: () => call("cmd_modules"),
  readMem: (address, size = 256) => call("cmd_read_mem", { address, size }),
  writeMem: (address, data) => call("cmd_write_mem", { address, data }),
  resolve: (addresses) => call("cmd_resolve", { addresses }),
};
