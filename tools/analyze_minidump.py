#!/usr/bin/env python3
"""Small dependency-free minidump triage helper.

It intentionally performs conservative stack-word scanning instead of claiming
to be a Windows unwinder.  That is enough to identify the wait routine and the
modules represented on each captured stack when WinDbg is unavailable.
"""

from __future__ import annotations

import argparse
import bisect
import ctypes
import pathlib
import struct
from dataclasses import dataclass


THREAD_LIST_STREAM = 3
MODULE_LIST_STREAM = 4
MEMORY64_LIST_STREAM = 9


@dataclass(frozen=True)
class Module:
    base: int
    size: int
    name: str

    @property
    def end(self) -> int:
        return self.base + self.size


class Dump:
    def __init__(self, path: pathlib.Path) -> None:
        self.data = path.read_bytes()
        signature, _version, count, directory_rva = struct.unpack_from(
            "<IIII", self.data, 0
        )
        if signature != 0x504D444D:
            raise ValueError("not a minidump")
        self.streams: dict[int, tuple[int, int]] = {}
        for index in range(count):
            kind, size, rva = struct.unpack_from(
                "<III", self.data, directory_rva + index * 12
            )
            if kind:
                self.streams[kind] = (rva, size)
        self.memory64 = self._read_memory64()
        self.modules = self._read_modules()
        self._exception_tables: dict[int, tuple[list[int], list[tuple[int, int, int]]]] = {}

    def _read_memory64(self) -> list[tuple[int, int, int]]:
        rva, _size = self.streams[MEMORY64_LIST_STREAM]
        count, file_rva = struct.unpack_from("<QQ", self.data, rva)
        ranges: list[tuple[int, int, int]] = []
        cursor = rva + 16
        for _ in range(count):
            address, size = struct.unpack_from("<QQ", self.data, cursor)
            ranges.append((address, address + size, file_rva))
            file_rva += size
            cursor += 16
        return ranges

    def read_virtual(self, address: int, size: int) -> bytes:
        for start, end, file_rva in self.memory64:
            if start <= address < end:
                size = min(size, end - address)
                offset = file_rva + address - start
                return self.data[offset : offset + size]
        return b""

    def _read_string(self, rva: int) -> str:
        length = struct.unpack_from("<I", self.data, rva)[0]
        return self.data[rva + 4 : rva + 4 + length].decode(
            "utf-16-le", errors="replace"
        )

    def _read_modules(self) -> list[Module]:
        rva, _size = self.streams[MODULE_LIST_STREAM]
        count = struct.unpack_from("<I", self.data, rva)[0]
        cursor = rva + 4
        modules: list[Module] = []
        for _ in range(count):
            base, size, _checksum, _stamp, name_rva = struct.unpack_from(
                "<QIIII", self.data, cursor
            )
            modules.append(Module(base, size, self._read_string(name_rva)))
            cursor += 108
        return sorted(modules, key=lambda module: module.base)

    def module_at(self, address: int) -> str:
        for module in self.modules:
            if module.base <= address < module.end:
                return f"{pathlib.PureWindowsPath(module.name).name}+0x{address - module.base:x}"
        return "<private>"

    def module_for(self, address: int) -> Module | None:
        for module in self.modules:
            if module.base <= address < module.end:
                return module
        return None

    def runtime_function(self, address: int) -> tuple[Module, tuple[int, int, int]] | None:
        module = self.module_for(address)
        if module is None:
            return None
        table = self._exception_tables.get(module.base)
        if table is None:
            header = self.read_virtual(module.base, 0x1000)
            if len(header) < 0x100:
                return None
            pe_offset = struct.unpack_from("<I", header, 0x3C)[0]
            optional = pe_offset + 24
            magic = struct.unpack_from("<H", header, optional)[0]
            directory = optional + (112 if magic == 0x20B else 96)
            exception_rva, exception_size = struct.unpack_from(
                "<II", header, directory + 3 * 8
            )
            raw = self.read_virtual(module.base + exception_rva, exception_size)
            entries = [
                struct.unpack_from("<III", raw, offset)
                for offset in range(0, len(raw) - 11, 12)
            ]
            table = ([entry[0] for entry in entries], entries)
            self._exception_tables[module.base] = table
        begins, entries = table
        rva = address - module.base
        index = bisect.bisect_right(begins, rva) - 1
        if index >= 0 and entries[index][0] <= rva < entries[index][1]:
            return module, entries[index]
        return None

    def threads(self) -> list[tuple[int, int, int, bytes]]:
        rva, _size = self.streams[THREAD_LIST_STREAM]
        count = struct.unpack_from("<I", self.data, rva)[0]
        cursor = rva + 4
        threads: list[tuple[int, int, int, bytes]] = []
        for _ in range(count):
            thread_id = struct.unpack_from("<I", self.data, cursor)[0]
            context_size, context_rva = struct.unpack_from(
                "<II", self.data, cursor + 40
            )
            if context_size < 256:
                cursor += 48
                continue
            # AMD64 CONTEXT: RSP at +0x98 and RIP at +0xf8.
            rsp = struct.unpack_from("<Q", self.data, context_rva + 0x98)[0]
            rip = struct.unpack_from("<Q", self.data, context_rva + 0xF8)[0]
            threads.append(
                (thread_id, rip, rsp, self.data[context_rva : context_rva + context_size])
            )
            cursor += 48
        return threads


class Address64(ctypes.Structure):
    _fields_ = [
        ("Offset", ctypes.c_ulonglong),
        ("Segment", ctypes.c_ushort),
        ("Mode", ctypes.c_int),
    ]


class KdHelp64(ctypes.Structure):
    _fields_ = [
        ("Thread", ctypes.c_ulonglong),
        ("ThCallbackStack", ctypes.c_ulong),
        ("ThCallbackBStore", ctypes.c_ulong),
        ("NextCallback", ctypes.c_ulong),
        ("FramePointer", ctypes.c_ulong),
        ("KiCallUserMode", ctypes.c_ulonglong),
        ("KeUserCallbackDispatcher", ctypes.c_ulonglong),
        ("SystemRangeStart", ctypes.c_ulonglong),
        ("KiUserExceptionDispatcher", ctypes.c_ulonglong),
        ("StackBase", ctypes.c_ulonglong),
        ("StackLimit", ctypes.c_ulonglong),
        ("BuildVersion", ctypes.c_ulong),
        ("RetpolineStubFunctionTableSize", ctypes.c_ulong),
        ("RetpolineStubFunctionTable", ctypes.c_ulonglong),
        ("RetpolineStubOffset", ctypes.c_ulong),
        ("RetpolineStubSize", ctypes.c_ulong),
        ("Reserved0", ctypes.c_ulonglong * 2),
    ]


class StackFrame64(ctypes.Structure):
    _fields_ = [
        ("AddrPC", Address64),
        ("AddrReturn", Address64),
        ("AddrFrame", Address64),
        ("AddrStack", Address64),
        ("AddrBStore", Address64),
        ("FuncTableEntry", ctypes.c_void_p),
        ("Params", ctypes.c_ulonglong * 4),
        ("Far", ctypes.c_int),
        ("Virtual", ctypes.c_int),
        ("Reserved", ctypes.c_ulonglong * 3),
        ("KdHelp", KdHelp64),
    ]


class SymbolInfo(ctypes.Structure):
    _fields_ = [
        ("SizeOfStruct", ctypes.c_ulong),
        ("TypeIndex", ctypes.c_ulong),
        ("Reserved", ctypes.c_ulonglong * 2),
        ("Index", ctypes.c_ulong),
        ("Size", ctypes.c_ulong),
        ("ModBase", ctypes.c_ulonglong),
        ("Flags", ctypes.c_ulong),
        ("Value", ctypes.c_ulonglong),
        ("Address", ctypes.c_ulonglong),
        ("Register", ctypes.c_ulong),
        ("Scope", ctypes.c_ulong),
        ("Tag", ctypes.c_ulong),
        ("NameLen", ctypes.c_ulong),
        ("MaxNameLen", ctypes.c_ulong),
        ("Name", ctypes.c_char * 1),
    ]


class RuntimeFunction(ctypes.Structure):
    _fields_ = [
        ("BeginAddress", ctypes.c_ulong),
        ("EndAddress", ctypes.c_ulong),
        ("UnwindData", ctypes.c_ulong),
    ]


class Unwinder:
    MACHINE_AMD64 = 0x8664
    FLAT = 3

    def __init__(self, dump: Dump, symbol_paths: list[pathlib.Path]) -> None:
        self.dump = dump
        self.process = ctypes.c_void_p(1)
        self.dbghelp = ctypes.WinDLL("dbghelp.dll", use_last_error=True)
        self.dbghelp.SymInitializeW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_int]
        self.dbghelp.SymInitializeW.restype = ctypes.c_int
        self.dbghelp.SymSetOptions.argtypes = [ctypes.c_ulong]
        self.dbghelp.SymSetOptions.restype = ctypes.c_ulong
        self.dbghelp.SymLoadModuleExW.argtypes = [
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_wchar_p,
            ctypes.c_wchar_p,
            ctypes.c_ulonglong,
            ctypes.c_ulong,
            ctypes.c_void_p,
            ctypes.c_ulong,
        ]
        self.dbghelp.SymLoadModuleExW.restype = ctypes.c_ulonglong
        self.dbghelp.SymFromAddr.argtypes = [
            ctypes.c_void_p,
            ctypes.c_ulonglong,
            ctypes.POINTER(ctypes.c_ulonglong),
            ctypes.c_void_p,
        ]
        self.dbghelp.SymFromAddr.restype = ctypes.c_int
        self.dbghelp.SymFunctionTableAccess64.argtypes = [
            ctypes.c_void_p,
            ctypes.c_ulonglong,
        ]
        self.dbghelp.SymFunctionTableAccess64.restype = ctypes.c_void_p
        self.dbghelp.SymGetModuleBase64.argtypes = [
            ctypes.c_void_p,
            ctypes.c_ulonglong,
        ]
        self.dbghelp.SymGetModuleBase64.restype = ctypes.c_ulonglong
        self.dbghelp.StackWalk64.argtypes = [
            ctypes.c_ulong,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.POINTER(StackFrame64),
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_void_p,
        ]
        self.dbghelp.StackWalk64.restype = ctypes.c_int
        self.dbghelp.SymSetOptions(0x00000002 | 0x00000004 | 0x00000010 | 0x00000200)
        search_path = ";".join(str(path.resolve()) for path in symbol_paths)
        if not self.dbghelp.SymInitializeW(self.process, search_path, False):
            raise OSError(ctypes.get_last_error(), "SymInitializeW")
        self.loaded: list[tuple[str, int, int]] = []
        for module in dump.modules:
            path = pathlib.Path(module.name)
            if not path.exists():
                continue
            loaded = self.dbghelp.SymLoadModuleExW(
                self.process,
                None,
                str(path),
                None,
                module.base,
                module.size,
                None,
                0,
            )
            self.loaded.append((path.name, loaded, ctypes.get_last_error()))
        read_type = ctypes.WINFUNCTYPE(
            ctypes.c_int,
            ctypes.c_void_p,
            ctypes.c_ulonglong,
            ctypes.c_void_p,
            ctypes.c_ulong,
            ctypes.POINTER(ctypes.c_ulong),
        )
        self.read_requests: list[tuple[int, int, int]] = []

        def read_memory(_process, address, buffer, size, count) -> int:
            data = dump.read_virtual(address, size)
            if len(self.read_requests) < 100:
                self.read_requests.append((address, size, len(data)))
            if not data:
                count[0] = 0
                return False
            ctypes.memmove(buffer, data, len(data))
            count[0] = len(data)
            return bool(data)

        self.read_memory = read_type(read_memory)
        function_type = ctypes.WINFUNCTYPE(
            ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ulonglong
        )
        module_type = ctypes.WINFUNCTYPE(
            ctypes.c_ulonglong, ctypes.c_void_p, ctypes.c_ulonglong
        )
        self.runtime_entries: dict[tuple[int, int], RuntimeFunction] = {}
        self.function_requests: list[int] = []

        def function_table(_process, address):
            if len(self.function_requests) < 100:
                self.function_requests.append(address)
            found = dump.runtime_function(address)
            if found is None:
                return None
            module, values = found
            key = (module.base, values[0])
            entry = self.runtime_entries.get(key)
            if entry is None:
                entry = RuntimeFunction(*values)
                self.runtime_entries[key] = entry
            return ctypes.addressof(entry)

        def module_base(_process, address):
            module = dump.module_for(address)
            return module.base if module is not None else 0

        self.function_table = function_type(function_table)
        self.module_base = module_type(module_base)

    def symbol(self, address: int) -> str:
        storage = ctypes.create_string_buffer(ctypes.sizeof(SymbolInfo) + 1024)
        info = ctypes.cast(storage, ctypes.POINTER(SymbolInfo)).contents
        info.SizeOfStruct = ctypes.sizeof(SymbolInfo)
        info.MaxNameLen = 1024
        displacement = ctypes.c_ulonglong()
        if not self.dbghelp.SymFromAddr(
            self.process, address, ctypes.byref(displacement), storage
        ):
            return self.dump.module_at(address)
        name_address = ctypes.addressof(storage) + SymbolInfo.Name.offset
        name = ctypes.string_at(name_address, info.NameLen).decode(errors="replace")
        return f"{name}+0x{displacement.value:x}"

    def unwind(self, context_bytes: bytes, limit: int = 64) -> tuple[list[tuple[int, int]], int]:
        context = ctypes.create_string_buffer(context_bytes)
        rip = struct.unpack_from("<Q", context_bytes, 0xF8)[0]
        rsp = struct.unpack_from("<Q", context_bytes, 0x98)[0]
        rbp = struct.unpack_from("<Q", context_bytes, 0xA0)[0]
        frame = StackFrame64()
        frame.AddrPC = Address64(rip, 0, self.FLAT)
        frame.AddrStack = Address64(rsp, 0, self.FLAT)
        frame.AddrFrame = Address64(rbp, 0, self.FLAT)
        frames = [(rip, rsp)]
        last_error = 0
        for _ in range(limit - 1):
            ctypes.set_last_error(0)
            ok = self.dbghelp.StackWalk64(
                self.MACHINE_AMD64,
                self.process,
                None,
                ctypes.byref(frame),
                context,
                self.read_memory,
                self.function_table,
                self.module_base,
                None,
            )
            current = (frame.AddrPC.Offset, frame.AddrStack.Offset)
            changed = current != frames[-1] and bool(current[0])
            if changed:
                frames.append(current)
            if not ok or not frame.AddrPC.Offset:
                last_error = ctypes.get_last_error()
                break
            if not changed:
                break
        return frames, last_error


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("dump", type=pathlib.Path)
    parser.add_argument("--stack-bytes", type=lambda value: int(value, 0), default=0x600)
    parser.add_argument("--unwind", action="store_true")
    parser.add_argument("--disassemble", action="store_true")
    parser.add_argument("--registers", action="store_true")
    parser.add_argument("--symbol-path", action="append", type=pathlib.Path, default=[])
    args = parser.parse_args()
    dump = Dump(args.dump)

    print("modules:")
    for module in dump.modules:
        name = pathlib.PureWindowsPath(module.name).name
        print(f"  0x{module.base:016x}-0x{module.end:016x} {name}")

    print("threads:")
    unwinder = Unwinder(dump, args.symbol_path) if args.unwind else None
    if unwinder is not None:
        print("symbol modules:")
        for name, base, error in unwinder.loaded:
            print(f"  {name}: base=0x{base:x} last_error={error}")
    for thread_id, rip, rsp, context in dump.threads():
        print(
            f"  tid={thread_id} rip=0x{rip:016x} {dump.module_at(rip)} "
            f"rsp=0x{rsp:016x}"
        )
        if args.registers:
            register_offsets = {
                "rax": 0x78,
                "rcx": 0x80,
                "rdx": 0x88,
                "rbx": 0x90,
                "rsp": 0x98,
                "rbp": 0xA0,
                "rsi": 0xA8,
                "rdi": 0xB0,
                "r8": 0xB8,
                "r9": 0xC0,
                "r10": 0xC8,
                "r11": 0xD0,
                "r12": 0xD8,
                "r13": 0xE0,
                "r14": 0xE8,
                "r15": 0xF0,
                "rip": 0xF8,
            }
            values = [
                f"{name}=0x{struct.unpack_from('<Q', context, offset)[0]:x}"
                for name, offset in register_offsets.items()
            ]
            print("    " + " ".join(values[:8]))
            print("    " + " ".join(values[8:]))
        if args.disassemble:
            from capstone import Cs, CS_ARCH_X86, CS_MODE_64

            decoder = Cs(CS_ARCH_X86, CS_MODE_64)
            for instruction in list(decoder.disasm(dump.read_virtual(rip, 48), rip))[:8]:
                print(
                    f"    0x{instruction.address:016x}: "
                    f"{instruction.mnemonic:<8} {instruction.op_str}"
                )
        stack = dump.read_virtual(rsp, args.stack_bytes)
        seen: set[tuple[str, int]] = set()
        candidates: list[str] = []
        for offset in range(0, len(stack) - 7, 8):
            value = struct.unpack_from("<Q", stack, offset)[0]
            location = dump.module_at(value)
            if location == "<private>":
                continue
            module_name = location.split("+", 1)[0]
            key = (module_name.lower(), value)
            if key in seen:
                continue
            seen.add(key)
            resolved = unwinder.symbol(value) if unwinder is not None else location
            candidates.append(f"[rsp+0x{offset:x}]=0x{value:x} {resolved}")
            if len(candidates) == 20:
                break
        for candidate in candidates:
            print(f"    {candidate}")
        if unwinder is not None:
            print("    unwind:")
            frames, error = unwinder.unwind(context)
            for index, (address, stack_pointer) in enumerate(frames):
                print(
                    f"      #{index:02d} 0x{address:016x} "
                    f"{unwinder.symbol(address)} rsp=0x{stack_pointer:016x}"
                )
            print(f"      stackwalk_error={error}")
            if unwinder.read_requests:
                print(
                    "      reads="
                    + ", ".join(
                        f"0x{address:x}/{asked}/{received}"
                        for address, asked, received in unwinder.read_requests[-8:]
                    )
                )
            if unwinder.function_requests:
                print(
                    "      functions="
                    + ", ".join(
                        f"0x{address:x}" for address in unwinder.function_requests[-8:]
                    )
                )


if __name__ == "__main__":
    main()
