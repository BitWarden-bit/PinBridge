"""VMProtect OEP strategy built only from generic PinBridge primitives.

This is deliberately an external dynamic-analysis policy, not debugger core:

1. Parse the main image's in-memory PE headers and locate `.text`.
2. Resolve the native number of NtProtectVirtualMemory from ntdll's syscall
   stub instead of hard-coding an OS-build-specific ordinal.
3. Synchronously observe only that syscall's exit. While its application
   thread waits for Python, query `.text` protection without loopback RPC.
4. After `.text` has been writable and returns to non-writable executable
   protection, arm a generic one-shot execution range trap.
5. Pin stops before the first instruction in `.text` executes and publishes
   `execution.trap` after all application contexts are stable.

Load this early (normally while the launcher entry breakpoint is stopped):
    pinbridge-cli --port <port> script run examples/python/vmp_oep.py

The target remains stopped on the candidate. Dumping is a separate strategy
stage and is intentionally not performed here.
"""

import pb


PAGE_WRITE_MASK = 0xCC
PAGE_EXECUTE_MASK = 0xF0
MEM_COMMIT = 0x1000

image_base = 0
image_end = 0
text_start = 0
text_end = 0
protect_syscall = None
saw_write = False
trap_id = None
oep = None
last_protect = None
initialized = False


def u16(data, offset):
    return int.from_bytes(data[offset:offset + 2], "little")


def u32(data, offset):
    return int.from_bytes(data[offset:offset + 4], "little")


def main_image():
    for low, end, is_main, name in pb.modules():
        if is_main:
            region = pb.memory_region(low)
            if region is None:
                raise RuntimeError("main image low address is not mapped")
            allocation_base = region[2]
            pb.print(
                "vmp_oep: main image low=0x%x allocation_base=0x%x end=0x%x name=%s"
                % (low, allocation_base, end, name)
            )
            return allocation_base or low, end, name
    raise RuntimeError("main image is not loaded")


def pe_text_range(base):
    first = pb.read_mem(base, 0x1000)
    if not first or len(first) < 0x100 or first[:2] != b"MZ":
        prefix = "none" if not first else first[:16].hex()
        raise RuntimeError(
            "main image has no readable DOS header at 0x%x len=%d prefix=%s"
            % (base, 0 if not first else len(first), prefix)
        )
    pe_offset = u32(first, 0x3C)
    needed = pe_offset + 24
    if needed > len(first):
        first = pb.read_mem(base, needed + 0x1000)
    if not first or first[pe_offset:pe_offset + 4] != b"PE\0\0":
        raise RuntimeError("main image has no readable PE header")
    section_count = u16(first, pe_offset + 6)
    optional_size = u16(first, pe_offset + 20)
    table = pe_offset + 24 + optional_size
    needed = table + section_count * 40
    if needed > len(first):
        first = pb.read_mem(base, needed)
    if not first or len(first) < needed:
        raise RuntimeError("main image section table is truncated")
    for index in range(section_count):
        row = table + index * 40
        name = first[row:row + 8].split(b"\0", 1)[0]
        if name != b".text":
            continue
        virtual_size = u32(first, row + 8)
        virtual_address = u32(first, row + 12)
        raw_size = u32(first, row + 16)
        size = max(virtual_size, raw_size)
        if not size:
            raise RuntimeError("main image .text section is empty")
        return base + virtual_address, base + virtual_address + size
    raise RuntimeError("main image has no .text section")


def syscall_number(export_address):
    code = pb.read_mem(export_address, 64)
    if not code:
        raise RuntimeError("cannot read NtProtectVirtualMemory syscall stub")
    # Native x64 and x86 ntdll stubs both load the service ordinal with
    # `mov eax, imm32`. Require a nearby syscall/sysenter/int 2e marker so a
    # coincidental B8 byte in a detour is not accepted.
    markers = (b"\x0f\x05", b"\x0f\x34", b"\xcd\x2e")
    for index in range(0, min(len(code) - 5, 32)):
        if code[index] != 0xB8:
            continue
        tail = code[index + 5:index + 32]
        if any(marker in tail for marker in markers):
            return u32(code, index + 1) & 0xFFF
    raise RuntimeError("unrecognized NtProtectVirtualMemory syscall stub")


def protection_snapshot():
    region = pb.memory_region(text_start)
    if region is None:
        return None
    _base, _size, allocation_base, protect, state, kind = region
    if allocation_base != image_base or state != MEM_COMMIT:
        return None
    return protect, kind


def on_protect_exit(event):
    global saw_write, trap_id, last_protect
    snapshot = protection_snapshot()
    if snapshot is None:
        return None
    protect, _kind = snapshot
    if protect != last_protect:
        pb.print(
            "vmp_oep: .text protection %s -> 0x%x"
            % ("unknown" if last_protect is None else "0x%x" % last_protect, protect)
        )
        last_protect = protect
    writable = protect & PAGE_WRITE_MASK != 0
    executable = protect & PAGE_EXECUTE_MASK != 0
    if writable:
        saw_write = True
    finalized = saw_write and executable and not writable
    if finalized and trap_id is None:
        # This call is safe inside a synchronous syscall decision: it updates
        # native instrumentation directly and performs no loopback RPC.
        trap_id = pb.execution_trap(text_start, text_end, once=True)
        pb.print(
            "vmp_oep: LATE_ARM trap=%d range=0x%x-0x%x after protect=0x%x"
            % (trap_id, text_start, text_end, protect)
        )
    return None


def on_execution_trap(event):
    global oep
    if trap_id is None or event["id"] != trap_id:
        return
    oep = event["address"]
    tid = event["tid"]
    rva = oep - image_base
    ip_name = "rip" if oep > 0xFFFFFFFF else "eip"
    sp_name = "rsp" if oep > 0xFFFFFFFF else "esp"
    ip = pb.get_reg(tid, ip_name)
    sp = pb.get_reg(tid, sp_name)
    pb.print(
        "vmp_oep: HIT candidate VA=0x%x RVA=0x%x tid=%d %s=0x%x %s=0x%x"
        % (oep, rva, tid, ip_name, ip or 0, sp_name, sp or 0)
    )
    rows = pb.disasm(oep, 8) or []
    for address, _size, _kind, _target, text in rows:
        pb.print("vmp_oep:   0x%x  %s" % (address, text))
    pb.print("vmp_oep: target remains stopped; dump stage may now consume this context")


def initialize(_event):
    global initialized, image_base, image_end, text_start, text_end
    global protect_syscall, last_protect, saw_write
    if initialized:
        return
    image_base, image_end, image_name = main_image()
    text_start, text_end = pe_text_range(image_base)
    nt_protect = pb.resolve_name("ntdll.dll!NtProtectVirtualMemory")
    if not nt_protect:
        raise RuntimeError("cannot resolve ntdll!NtProtectVirtualMemory")
    protect_syscall = syscall_number(nt_protect)

    initial = protection_snapshot()
    if initial is not None:
        last_protect = initial[0]
        saw_write = last_protect & PAGE_WRITE_MASK != 0

    pb.intercept("syscall.exit", on_protect_exit, numbers=[protect_syscall])
    initialized = True
    pb.print(
        "vmp_oep: ready image=%s base=0x%x .text=0x%x-0x%x NtProtectVM=#0x%x initial_protect=%s"
        % (
            image_name,
            image_base,
            text_start,
            text_end,
            protect_syscall,
            "unknown" if last_protect is None else "0x%x" % last_protect,
        )
    )


# SCRIPT_LOAD keeps the query server parked until top-level code returns, so
# target-inspection RPCs are deliberately deferred to the sticky process.start
# replay. This also makes hot injection work after the process-start edge.
pb.on("execution.trap", on_execution_trap)
pb.on("process.start", initialize)
