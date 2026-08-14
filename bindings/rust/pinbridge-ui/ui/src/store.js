// Snapshot stream store: the agent pushes ~4 snapshots/sec over a Tauri
// event; components subscribe with useSyncExternalStore. The events table is
// capped so React never diffs more than 24 rows.

import { listen } from "@tauri-apps/api/event";

const state = {
  connected: false,
  abi: [0, 0],
  pid: 0,
  total: 0,
  dropped: 0,
  capacity: 0,
  kinds: [0, 0, 0, 0, 0, 0],
  rate: 0,
  rateHistory: [],
  events: [],
  stopped: false,
  hitTid: 0,
  hitAddr: "0x0",
  stopGen: 0,
  bps: [],
};

const listeners = new Set();

listen("snapshot", (e) => {
  const s = e.payload;
  if (!s.connected) {
    Object.assign(state, { connected: false });
  } else {
    state.connected = true;
    state.abi = s.abi;
    state.pid = s.pid;
    state.total = s.total;
    state.dropped = s.dropped;
    state.capacity = s.capacity;
    state.kinds = s.kinds;
    state.rate = s.rate;
    state.rateHistory = [...state.rateHistory.slice(-159), s.rate];
    state.events = s.events;
    state.stopped = !!s.stopped;
    // 0xFFFFFFFF = no hit (manual pause); tid 0 is a real thread
    state.hitTid = s.hit_tid ?? 0xffffffff;
    state.hitAddr = s.hit_addr || "0x0";
    state.stopGen = s.stop_gen ?? 0;
    state.bps = s.bps || [];
  }
  listeners.forEach((l) => l());
});

export function subscribe(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function getSnapshot() {
  return state;
}
