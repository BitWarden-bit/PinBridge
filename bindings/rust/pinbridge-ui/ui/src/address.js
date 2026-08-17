// Addresses stay as text across the UI. BigInt is used only for exact
// arithmetic; no address is ever converted through a JavaScript Number.
export function normalizeAddress(value) {
  if (value == null) return null;
  const text = String(value).trim();
  if (!text) return null;
  try {
    const number = /^0x/i.test(text)
      ? BigInt(text)
      : /^\d+$/.test(text)
        ? BigInt(text)
        : BigInt(`0x${text}`);
    if (number < 0n) return null;
    return `0x${number.toString(16)}`;
  } catch {
    return null;
  }
}

export function addAddress(value, delta) {
  const base = normalizeAddress(value);
  if (!base) return null;
  try {
    const next = BigInt(base) + BigInt(delta);
    return next < 0n ? null : `0x${next.toString(16)}`;
  } catch {
    return null;
  }
}
