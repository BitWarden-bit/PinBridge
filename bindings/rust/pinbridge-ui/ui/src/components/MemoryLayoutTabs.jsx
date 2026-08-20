import React, { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";

let cachedLayout = null;
let pendingLayout = null;

async function fetchLayout(force = false) {
  if (!force && cachedLayout) return cachedLayout;
  if (!force && pendingLayout) return pendingLayout;
  pendingLayout = api.memoryMap().then((result) => {
    if (!result.ok) throw new Error(result.error || "读取内存布局失败");
    cachedLayout = normalizeLayout(result.value);
    return cachedLayout;
  }).finally(() => {
    pendingLayout = null;
  });
  return pendingLayout;
}

function useMemoryLayout(stopTick) {
  const [layout, setLayout] = useState(cachedLayout);
  const [loading, setLoading] = useState(!cachedLayout);
  const [error, setError] = useState("");

  const refresh = async (force = true) => {
    setLoading(true);
    setError("");
    try {
      setLayout(await fetchLayout(force));
    } catch (reason) {
      setError(String(reason?.message || reason));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    refresh(true);
  }, [stopTick]);

  return { layout, loading, error, refresh };
}

export function ModulesTab({ stopTick, onGoto }) {
  const { layout, loading, error, refresh } = useMemoryLayout(stopTick);
  const [query, setQuery] = useState("");
  const [selectedBase, setSelectedBase] = useState("");
  const modules = layout?.modules || [];
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return modules;
    return modules.filter((module) =>
      `${module.name} ${module.base} ${module.entry} ${module.sections.map((section) => section.name).join(" ")}`
        .toLowerCase()
        .includes(needle),
    );
  }, [modules, query]);
  const selected = modules.find((module) => module.base === selectedBase)
    || filtered.find((module) => module.isMain)
    || filtered[0]
    || null;

  return (
    <div className="pbl-root">
      <LayoutToolbar
        query={query}
        setQuery={setQuery}
        placeholder="搜索模块、路径、区段…"
        loading={loading}
        error={error}
        onRefresh={() => refresh(true)}
        summary={`${modules.length} 个模块 · ${modules.reduce((count, module) => count + module.sections.length, 0)} 个区段`}
      />
      <div className="pbl-split">
        <div className="pbl-table-wrap">
          <table className="pbl-table pbl-modules-table">
            <thead><tr><th /><th>模块</th><th>基址</th><th>映射大小</th><th>入口点</th><th>区段</th></tr></thead>
            <tbody>
              {filtered.map((module) => (
                <tr
                  key={module.base}
                  className={selected?.base === module.base ? "selected" : ""}
                  onClick={() => setSelectedBase(module.base)}
                  onDoubleClick={() => onGoto(module.base)}
                >
                  <td className="pbl-main-mark">{module.isMain ? "●" : ""}</td>
                  <td><b>{shortName(module.name)}</b><small>{module.name}</small></td>
                  <td className="mono addr">{displayAddress(module.base)}</td>
                  <td className="mono">{formatSize(module.mappedSize || span(module.base, module.end))}</td>
                  <td className="mono addr">{displayAddress(module.entry)}</td>
                  <td>{module.sections.length}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {!loading && filtered.length === 0 && <div className="pbl-empty">没有匹配的模块</div>}
        </div>
        <ModuleDetail module={selected} onGoto={onGoto} />
      </div>
    </div>
  );
}

function ModuleDetail({ module, onGoto }) {
  if (!module) return <aside className="pbl-detail"><div className="pbl-empty">选择模块查看详细布局</div></aside>;
  return (
    <aside className="pbl-detail">
      <header><div><b>{shortName(module.name)}</b><span>{module.isMain ? "主模块" : "加载模块"}</span></div><button onClick={() => onGoto(module.base)}>转到基址</button></header>
      <div className="pbl-kv"><span>完整路径</span><code title={module.name}>{module.name}</code></div>
      <div className="pbl-kv-grid">
        <div><span>基址</span><code>{displayAddress(module.base)}</code></div>
        <div><span>结束</span><code>{displayAddress(module.end)}</code></div>
        <div><span>入口点</span><code>{displayAddress(module.entry)}</code></div>
        <div><span>映射大小</span><code>{formatSize(module.mappedSize)}</code></div>
      </div>
      <h3>区段 <em>{module.sections.length}</em></h3>
      <div className="pbl-section-list">
        <table className="pbl-table">
          <thead><tr><th>名称</th><th>范围</th><th>大小</th><th>属性</th></tr></thead>
          <tbody>{module.sections.map((section, index) => (
            <tr key={`${section.address}-${index}`} onDoubleClick={() => onGoto(section.address)}>
              <td><b>{section.name || `(section ${index})`}</b></td>
              <td className="mono addr">{displayAddress(section.address)}–{displayAddress(addHex(section.address, section.size))}</td>
              <td className="mono">{formatSize(section.size)}</td>
              <td><code className="pbl-flags">{sectionFlags(section)}</code></td>
            </tr>
          ))}</tbody>
        </table>
      </div>
    </aside>
  );
}

export function MemoryMapTab({ stopTick, onGoto }) {
  const { layout, loading, error, refresh } = useMemoryLayout(stopTick);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("image");
  const [view, setView] = useState("sections");
  const [selectedKey, setSelectedKey] = useState("");
  const tableWrapRef = useRef(null);
  const pages = useMemo(() => enrichPages(layout), [layout]);
  const rows = useMemo(
    () => view === "sections" ? buildSectionView(layout, pages) : pages,
    [layout, pages, view],
  );
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return rows.filter((row) => {
      if (filter !== "all" && row.category !== filter) return false;
      if (!needle) return true;
      return `${row.address} ${row.info} ${row.party} ${row.typeShort} ${row.currentProtection} ${row.initialProtection}`
        .toLowerCase()
        .includes(needle);
    });
  }, [rows, query, filter]);
  const selected = rows.find((row) => row.key === selectedKey) || filtered[0] || null;
  const committed = pages.filter((row) => row.state === 0x1000).reduce((total, row) => total + toBigInt(row.size), 0n);

  useEffect(() => {
    if (tableWrapRef.current) tableWrapRef.current.scrollTop = 0;
  }, [filter, view]);

  return (
    <div className="pbl-root pbl-x64map">
      <LayoutToolbar
        query={query}
        setQuery={setQuery}
        placeholder="搜索模块、区段、Heap、地址…"
        loading={loading}
        error={error}
        onRefresh={() => refresh(true)}
        summary={`${rows.length} 项 · ${layout?.heaps.length || 0} 个 Heap · 已提交 ${formatSize(committed)}`}
      >
        <select value={filter} onChange={(event) => setFilter(event.target.value)}>
          <option value="all">全部</option>
          <option value="image">模块映像</option>
          <option value="heap">Heap</option>
          <option value="private">Private</option>
          <option value="mapped">Mapped</option>
          <option value="reserve">Reserved</option>
        </select>
        <div className="pbl-view-switch" aria-label="内存布局视图">
          <button className={view === "sections" ? "active" : ""} onClick={() => setView("sections")}>区段</button>
          <button className={view === "pages" ? "active" : ""} onClick={() => setView("pages")}>页面</button>
        </div>
      </LayoutToolbar>
      <div className="pbl-split pbl-map-split">
        <div className="pbl-table-wrap" ref={tableWrapRef}>
          <table className="pbl-table pbl-map-table">
            <thead><tr><th>Address</th><th>Size</th><th>Party</th><th>Info</th><th>Type</th><th>Protection</th><th>Initial</th></tr></thead>
            <tbody>{filtered.map((row) => (
              <tr
                key={row.key}
                className={`${selected?.key === row.key ? "selected" : ""} ${row.executable ? "executable" : ""} ${row.moduleHeader ? "module-header" : ""}`}
                onClick={() => setSelectedKey(row.key)}
                onDoubleClick={() => onGoto(row.address)}
              >
                <td className="mono addr" title={row.address}>{displayAddress(row.address)}</td>
                <td className="mono" title={formatSize(row.size)}>{displaySize(row.size)}</td>
                <td><span className={`pbl-party ${row.party === "系统" ? "system" : "user"}`}>{row.party}</span></td>
                <td className="pbl-map-info" title={row.info}><b>{row.info || "—"}</b></td>
                <td>{row.typeShort}</td>
                <td><code className="pbl-memory-rights">{row.currentProtection}</code></td>
                <td><code className="pbl-memory-rights initial">{row.initialProtection}</code></td>
              </tr>
            ))}</tbody>
          </table>
          {!loading && filtered.length === 0 && <div className="pbl-empty">没有匹配的内存区域</div>}
        </div>
        <RegionDetail row={selected} onGoto={onGoto} />
      </div>
    </div>
  );
}

function RegionDetail({ row, onGoto }) {
  if (!row) return <aside className="pbl-detail"><div className="pbl-empty">选择一项查看模块与内存关系</div></aside>;
  return (
    <aside className="pbl-detail pbl-region-detail">
      <header>
        <div><b>{row.info || row.typeName}</b><span>{row.typeName} · {row.stateName}</span></div>
        <button onClick={() => onGoto(row.address)}>转到内存</button>
      </header>
      <div className="pbl-kv-grid">
        <div><span>Address</span><code>{displayAddress(row.address)}</code></div>
        <div><span>End</span><code>{displayAddress(addHex(row.address, row.size))}</code></div>
        <div><span>Size</span><code>{displaySize(row.size)} · {formatSize(row.size)}</code></div>
        <div><span>Allocation base</span><code>{displayAddress(row.allocationBase)}</code></div>
        <div><span>Protection</span><code>{row.currentProtection || "—"}</code></div>
        <div><span>Initial</span><code>{row.initialProtection || "—"}</code></div>
      </div>
      <div className="pbl-detail-relations">
        <div><span>归属</span><b>{row.party} · {row.typeShort} · {row.stateName}</b></div>
        {row.module && <div><span>模块</span><b title={row.module.name}>{shortName(row.module.name)} · {displayAddress(row.module.base)}</b></div>}
        {row.section && <div><span>区段</span><b>{row.section.name || "未命名区段"} · {sectionFlags(row.section)}</b></div>}
        {row.heap && <div><span>进程堆</span><b>Heap ID {row.heap.index} · root {displayAddress(row.heap.address)}</b></div>}
      </div>
    </aside>
  );
}

function LayoutToolbar({ query, setQuery, placeholder, loading, error, onRefresh, summary, children }) {
  return (
    <div className="pbl-toolbar">
      <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={placeholder} spellCheck="false" />
      {children}
      <button onClick={onRefresh} disabled={loading}>{loading ? "读取中…" : "刷新"}</button>
      <span>{error ? <em>{error}</em> : summary}</span>
    </div>
  );
}

function normalizeLayout(value) {
  const modules = (Array.isArray(value?.modules) ? value.modules : []).map((module) => ({
    base: normalizeHex(module.base),
    end: normalizeHex(module.end),
    entry: normalizeHex(module.entry),
    mappedSize: normalizeHex(module.mapped_size),
    imageType: Number(module.image_type || 0),
    isMain: Boolean(module.is_main),
    name: String(module.name || ""),
    sections: (Array.isArray(module.sections) ? module.sections : []).map((section) => ({
      name: String(section.name || ""),
      address: normalizeHex(section.address),
      size: normalizeHex(section.size),
      kind: Number(section.kind || 0),
      readable: Boolean(section.readable),
      writable: Boolean(section.writable),
      executable: Boolean(section.executable),
      mapped: Boolean(section.mapped),
    })).sort((a, b) => compareHex(a.address, b.address)),
  })).sort((a, b) => compareHex(a.base, b.base));
  const regions = (Array.isArray(value?.regions) ? value.regions : []).map((region) => ({
    base: normalizeHex(region.base),
    size: normalizeHex(region.size),
    allocationBase: normalizeHex(region.allocation_base),
    allocationProtect: parseNumeric(region.allocation_protect),
    protect: parseNumeric(region.protect),
    state: parseNumeric(region.state),
    type: parseNumeric(region.type),
  })).sort((a, b) => compareHex(a.base, b.base));
  const heaps = (Array.isArray(value?.heaps) ? value.heaps : []).map(normalizeHex).sort(compareHex);
  return { modules, regions, heaps };
}

function enrichPages(layout) {
  if (!layout) return [];
  const heapAllocations = new Map();
  layout.heaps.forEach((address, index) => {
    const root = layout.regions.find((region) => contains(region.base, region.size, address));
    if (root) heapAllocations.set(root.allocationBase, { index, address });
  });
  return layout.regions.map((region, index) => {
    const module = layout.modules.find((item) => overlaps(region.base, region.size, item.base, moduleSize(item))) || null;
    const section = module?.sections.find((item) => overlaps(region.base, region.size, item.address, item.size)) || null;
    const heap = heapAllocations.get(region.allocationBase) || null;
    let category = region.state === 0x2000 ? "reserve" : region.type === 0x40000 ? "mapped" : "private";
    let info = region.state === 0x2000
      ? `Reserved${region.base !== region.allocationBase ? ` (${displayAddress(region.allocationBase)})` : ""}`
      : "";
    if (module) {
      category = "image";
      info = toBigInt(region.base) === toBigInt(module.base)
        ? shortName(module.name)
        : section?.name || shortName(module.name);
    } else if (heap) {
      category = "heap";
      info = `Heap (ID ${heap.index})`;
    }
    return makeRow({
      key: `page-${region.base}-${index}`,
      address: region.base,
      size: region.size,
      allocationBase: region.allocationBase,
      allocationProtect: region.allocationProtect,
      protect: region.protect,
      state: region.state,
      type: region.type,
      category,
      info,
      module,
      section,
      heap,
      moduleHeader: module && toBigInt(region.base) === toBigInt(module.base),
    });
  });
}

function buildSectionView(layout, pages) {
  if (!layout) return [];
  const moduleRows = [];
  for (const module of layout.modules) {
    const modulePages = pages.filter((page) => overlaps(page.address, page.size, module.base, moduleSize(module)));
    if (module.sections.length === 0) {
      moduleRows.push(...modulePages);
      continue;
    }
    const firstSection = module.sections[0];
    const headerSize = toBigInt(firstSection.address) > toBigInt(module.base)
      ? toBigInt(firstSection.address) - toBigInt(module.base)
      : 0x1000n;
    moduleRows.push(rowForModulePart(module, null, module.base, headerSize, modulePages, true));
    module.sections.forEach((section, index) => {
      if (toBigInt(section.size) === 0n) return;
      moduleRows.push(rowForModulePart(
        module,
        section,
        section.address,
        section.size,
        modulePages,
        false,
        index,
      ));
    });
  }

  const nonModulePages = pages.filter((page) => !page.module);
  return [...moduleRows, ...collapseAllocations(nonModulePages)]
    .sort((a, b) => compareHex(a.address, b.address));
}

function rowForModulePart(module, section, address, size, pages, moduleHeader, index = -1) {
  const page = pages.find((item) => contains(item.address, item.size, address)) || pages[0] || {};
  return makeRow({
    key: `section-${module.base}-${section?.address || "header"}-${index}`,
    address,
    size: normalizeHex(size),
    allocationBase: module.base,
    allocationProtect: page.allocationProtect || 0,
    protect: page.protect || flagsToProtection(section),
    state: page.state || 0x1000,
    type: 0x1000000,
    category: "image",
    info: moduleHeader ? shortName(module.name) : section?.name || "section",
    module,
    section,
    heap: null,
    moduleHeader,
  });
}

function collapseAllocations(pages) {
  const collapsed = [];
  for (const page of pages) {
    const previous = collapsed[collapsed.length - 1];
    const canMerge = previous
      && page.state !== 0x2000
      && previous.state !== 0x2000
      && previous.allocationBase === page.allocationBase
      && toBigInt(previous.address) + toBigInt(previous.size) === toBigInt(page.address);
    if (!canMerge) {
      collapsed.push({ ...page, key: `allocation-${page.allocationBase}-${page.address}` });
      continue;
    }
    previous.size = normalizeHex(toBigInt(previous.size) + toBigInt(page.size));
    if (!previous.info && page.info) previous.info = page.info;
    if (!previous.heap && page.heap) previous.heap = page.heap;
    if (page.category === "heap") previous.category = "heap";
  }
  return collapsed;
}

function makeRow(row) {
  const typeName = typeNameFor(row.type);
  return {
    ...row,
    party: partyFor(row.module),
    typeName,
    typeShort: typeShortFor(row.type),
    stateName: stateNameFor(row.state),
    currentProtection: protectionName(row.protect),
    initialProtection: protectionName(row.allocationProtect),
    executable: Boolean((row.protect || 0) & 0xf0) || Boolean(row.section?.executable),
  };
}

function partyFor(module) {
  if (!module) return "用户";
  return /\\windows\\(?:system32|syswow64|winsxs)\\/i.test(module.name.replaceAll("/", "\\")) ? "系统" : "用户";
}

function protectionName(value) {
  if (!value) return "";
  const names = {
    0x01: "----", 0x02: "-R--", 0x04: "-RW-", 0x08: "-RWC",
    0x10: "E---", 0x20: "ER--", 0x40: "ERW-", 0x80: "ERWC",
  };
  return `${names[value & 0xff] || "????"}${value & 0x100 ? "G" : "-"}`;
}

function flagsToProtection(section) {
  if (!section) return 0;
  if (section.executable && section.writable) return 0x40;
  if (section.executable && section.readable) return 0x20;
  if (section.executable) return 0x10;
  if (section.writable) return 0x04;
  if (section.readable) return 0x02;
  return 0x01;
}

function stateNameFor(value) {
  if (value === 0x1000) return "Commit";
  if (value === 0x2000) return "Reserve";
  if (value === 0x10000) return "Free";
  return hex32(value);
}

function typeNameFor(value) {
  if (value === 0x1000000) return "Image";
  if (value === 0x40000) return "Mapped";
  if (value === 0x20000) return "Private";
  return value ? hex32(value) : "N/A";
}

function typeShortFor(value) {
  if (value === 0x1000000) return "IMG";
  if (value === 0x40000) return "MAP";
  if (value === 0x20000) return "PRV";
  return "N/A";
}

function sectionFlags(section) {
  return `${section.readable ? "R" : "-"}${section.writable ? "W" : "-"}${section.executable ? "X" : "-"}${section.mapped ? " M" : ""}`;
}

function moduleSize(module) {
  const mapped = toBigInt(module.mappedSize);
  if (mapped > 0n) return mapped;
  const start = toBigInt(module.base);
  const finish = toBigInt(module.end);
  return finish >= start ? finish - start + 1n : 0n;
}

function formatSize(value) {
  const size = typeof value === "bigint" ? value : toBigInt(value);
  if (size >= 1024n * 1024n * 1024n) return `${formatRatio(size, 1024n * 1024n * 1024n)} GiB`;
  if (size >= 1024n * 1024n) return `${formatRatio(size, 1024n * 1024n)} MiB`;
  if (size >= 1024n) return `${formatRatio(size, 1024n)} KiB`;
  return `${size} B`;
}

function formatRatio(value, divisor) {
  const whole = value / divisor;
  const tenth = (value % divisor) * 10n / divisor;
  return tenth ? `${whole}.${tenth}` : String(whole);
}

function displayAddress(value) {
  return toBigInt(value).toString(16).toUpperCase().padStart(16, "0");
}

function displaySize(value) {
  return toBigInt(value).toString(16).toUpperCase().padStart(16, "0");
}

function span(base, end) {
  const start = toBigInt(base);
  const finish = toBigInt(end);
  return finish > start ? finish - start : 0n;
}

function contains(base, size, address) {
  const start = toBigInt(base);
  const end = start + toBigInt(size);
  const point = toBigInt(address);
  return point >= start && point < end;
}

function overlaps(baseA, sizeA, baseB, sizeB) {
  const a0 = toBigInt(baseA);
  const a1 = a0 + toBigInt(sizeA);
  const b0 = toBigInt(baseB);
  const b1 = b0 + toBigInt(sizeB);
  return a0 < b1 && b0 < a1;
}

function addHex(base, size) {
  return `0x${(toBigInt(base) + toBigInt(size)).toString(16)}`;
}

function normalizeHex(value) {
  return `0x${toBigInt(value).toString(16)}`;
}

function toBigInt(value) {
  try {
    if (typeof value === "bigint") return value;
    return BigInt(value || 0);
  } catch {
    return 0n;
  }
}

function compareHex(a, b) {
  const left = toBigInt(a);
  const right = toBigInt(b);
  return left < right ? -1 : left > right ? 1 : 0;
}

function parseNumeric(value) {
  const parsed = Number.parseInt(String(value || "0"), 0);
  return Number.isFinite(parsed) ? parsed : 0;
}

function hex32(value) {
  return `0x${Number(value >>> 0).toString(16)}`;
}

function shortName(path) {
  return String(path || "").replaceAll("/", "\\").split("\\").pop() || "(unnamed)";
}
