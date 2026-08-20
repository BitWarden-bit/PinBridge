"""Bounded callback-Hook strategy for the live crypto.vmp.exe sample.

The entry callback is synchronous but observation-only. API callbacks are
asynchronous, address-filtered, and rate-limited. No register, argument,
return-value, or target-memory modifications are made.
"""

import pb


ENTRY_RVA = 0xE35662
LIVE_PROBE_ADDRESS = 0
LIVE_PROBE_THREAD_ID = None
PRINT_COUNTS = (1, 2, 3, 10, 100, 1000)

main_base = 0
main_end = 0
main_name = ""
entry_callback_id = 0
live_probe_callback_id = 0
api_callback_ids = []
armed_addresses = set()
hit_counts = {}


API_TARGETS = (
    ("GetProcAddress", ("kernel32.dll!GetProcAddress",)),
    ("LoadLibraryA", ("kernel32.dll!LoadLibraryA",)),
    ("VirtualProtect", ("kernel32.dll!VirtualProtect",)),
    ("VirtualAlloc", ("kernel32.dll!VirtualAlloc",)),
    ("ReadFile", ("kernel32.dll!ReadFile",)),
    ("WriteFile", ("kernel32.dll!WriteFile",)),
    ("NtProtectVirtualMemory", ("ntdll.dll!NtProtectVirtualMemory",)),
    ("NtAllocateVirtualMemory", ("ntdll.dll!NtAllocateVirtualMemory",)),
    ("BCryptEncrypt", ("bcrypt.dll!BCryptEncrypt",)),
    ("BCryptDecrypt", ("bcrypt.dll!BCryptDecrypt",)),
)


def _arguments(event):
    registers = event.get("registers", {})
    if "rcx" in registers:
        values = (
            registers.get("rcx", 0),
            registers.get("rdx", 0),
            registers.get("r8", 0),
            registers.get("r9", 0),
        )
    else:
        stack = event.get("stack_arguments", (0, 0, 0, 0))
        values = tuple(stack[:4]) + (0,) * max(0, 4 - len(stack))
    return values[:4]


def _make_api_callback(label):
    def observe(event):
        count = hit_counts.get(label, 0) + 1
        hit_counts[label] = count
        if count in PRINT_COUNTS:
            a0, a1, a2, a3 = _arguments(event)
            pb.print(
                "VMP_API_CALLBACK hit=%s count=%d tid=%d address=0x%x "
                "args=[0x%x,0x%x,0x%x,0x%x]"
                % (
                    label,
                    count,
                    event["tid"],
                    event["address"],
                    a0,
                    a1,
                    a2,
                    a3,
                )
            )

    return observe


def _arm_api_callbacks():
    for label, symbols in API_TARGETS:
        address = 0
        resolved = ""
        for symbol in symbols:
            address = pb.resolve_name(symbol) or 0
            if address:
                resolved = symbol
                break
        if not address or address in armed_addresses:
            continue
        callback_id = pb.on(
            "hook.entry", _make_api_callback(label), address=address, once=False
        )
        armed_addresses.add(address)
        api_callback_ids.append(callback_id)
        pb.print(
            "VMP_API_CALLBACK_ARMED id=%d api=%s symbol=%s address=0x%x"
            % (callback_id, label, resolved, address)
        )


def _observe_module_load(_event):
    # Some crypto providers are loaded lazily. The address set prevents a
    # module notification from registering duplicate native Hook leases.
    _arm_api_callbacks()


def _entry_callback(event):
    rows = pb.disasm(event["address"], 6) or []
    first = rows[0][4] if rows else "<decode unavailable>"
    registers = event.get("registers", {})
    stack_pointer = registers.get("rsp", registers.get("esp", 0))
    pb.print(
        "VMP_ENTRY_CALLBACK_HIT tid=%d address=0x%x sp=0x%x first=%s action=continue"
        % (event["tid"], event["address"], stack_pointer, first)
    )
    for address, size, _kind, _target, text in rows[:6]:
        pb.print("VMP_ENTRY_DISASM 0x%x size=%d %s" % (address, size, text))
    return {"action": "continue"}


def _live_probe_callback(event):
    rows = pb.disasm(event["address"], 4) or []
    first = rows[0][4] if rows else "<decode unavailable>"
    pb.print(
        "VMP_LIVE_CALLBACK_HIT tid=%d address=0x%x first=%s action=continue"
        % (event["tid"], event["address"], first)
    )
    return {"action": "continue"}


def pb_init():
    global main_base, main_end, main_name, entry_callback_id
    global live_probe_callback_id

    main = next((row for row in pb.modules() if row[2]), None)
    if main is None:
        raise RuntimeError("main module not found")
    main_base, main_end, _is_main, main_name = main
    entry = main_base + ENTRY_RVA
    if not (main_base <= entry < main_end):
        raise RuntimeError("configured entry RVA is outside the main module")

    entry_callback_id = pb.intercept(
        "hook.entry",
        _entry_callback,
        address=entry,
        once=True,
        description="observe the protected sample entry once and continue without modifying target state",
    )
    if LIVE_PROBE_ADDRESS:
        live_probe_callback_id = pb.intercept(
            "hook.entry",
            _live_probe_callback,
            address=LIVE_PROBE_ADDRESS,
            thread_id=LIVE_PROBE_THREAD_ID,
            once=True,
            description="prove one live callback hit at the operator-paused instruction and continue unchanged",
        )
        pb.print(
            "VMP_LIVE_CALLBACK_ARMED id=%d tid=%s address=0x%x"
            % (live_probe_callback_id, LIVE_PROBE_THREAD_ID, LIVE_PROBE_ADDRESS)
        )
    pb.on("module.load", _observe_module_load)
    _arm_api_callbacks()
    pb.print(
        "VMP_CALLBACK_STRATEGY_READY module=%s range=0x%x-0x%x "
        "entry=0x%x entry_callback=%d api_callbacks=%d"
        % (
            main_name,
            main_base,
            main_end,
            entry,
            entry_callback_id,
            len(api_callback_ids),
        )
    )


def on_unload():
    summary = ",".join(
        "%s=%d" % (label, hit_counts[label]) for label in sorted(hit_counts)
    )
    pb.print("VMP_CALLBACK_STRATEGY_UNLOAD hits=%s" % (summary or "none"))
