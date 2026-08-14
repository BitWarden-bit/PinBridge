// Address -> display-name resolver with a session-lifetime cache. Backed by
// the agent's RESOLVE opcode via cmd_resolve (module!export / module+0xoff).

import { api } from "./api";

const cache = new Map(); // "0x.." address string -> display string | null

// Resolves any not-yet-cached addresses in one batch; returns the cache.
export async function resolveInto(addrs) {
  const missing = [...new Set(addrs)].filter((a) => a && a !== "0x0" && !cache.has(a));
  if (missing.length) {
    const res = await api.resolve(missing);
    if (res) missing.forEach((a, i) => cache.set(a, res[i]));
  }
  return cache;
}

export function cachedName(addr) {
  return cache.get(addr) || null;
}

export function clearResolveCache() {
  cache.clear();
}
