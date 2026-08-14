#!/usr/bin/env python3
"""Generate Rust FFI bindings (pinbridge-sys) from the frozen PinBridge C ABI header.

Parses include/pinbridge/pinbridge.h plus the generated/*.inc files it includes.
The header is machine-regular, so this is a constrained parser, not a C compiler:

  - object-like #define constants (typed casts, UINT*_C, strings, small expressions)
  - function-like #define prototype generators expanded from the .inc files
    (PB_INS_QUERY0/1, PB_REG_QUERY0/1, PB_HANDLE_QUERY0/1)
  - typedef struct { ... } PbX;            -> #[repr(C)] struct
  - typedef struct PbXOpaque* PbXHandle;   -> opaque enum + pointer alias
  - typedef ret (PB_CALL* PbCallback)(..); -> Option<unsafe extern "C" fn(..)>
  - PB_API [PB_NORETURN] ret PB_CALL pb_*(params); -> unsafe extern "C" fn

Outputs:
  bindings/rust/pinbridge-sys/src/bindings.rs
  tests/expected_exports.txt   (every pb_* symbol the DLL must export)

Exits non-zero and prints ERROR: ... when a construct is not recognized, so a
header drift breaks generation loudly instead of silently dropping bindings.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HEADER = ROOT / "include" / "pinbridge" / "pinbridge.h"
OUT_RS = ROOT / "bindings" / "rust" / "pinbridge-sys" / "src" / "bindings.rs"
OUT_EXPORTS = ROOT / "tests" / "expected_exports.txt"

KEEP_TOKENS = {"PB_API", "PB_CALL", "PB_NORETURN"}

# ---------------------------------------------------------------------------
# C type -> Rust type
# ---------------------------------------------------------------------------

BASE_TYPES = {
    "void": "()",
    "char": "c_char",
    "int8_t": "i8",
    "int16_t": "i16",
    "int32_t": "i32",
    "int64_t": "i64",
    "uint8_t": "u8",
    "uint16_t": "u16",
    "uint32_t": "u32",
    "uint64_t": "u64",
    "size_t": "usize",
    "float": "f32",
    "double": "f64",
}

RUST_KEYWORDS = {
    "as", "async", "await", "box", "break", "const", "continue", "crate", "do",
    "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "if",
    "impl", "in", "let", "loop", "macro", "match", "mod", "move", "mut",
    "override", "priv", "pub", "ref", "return", "self", "Self", "static",
    "struct", "super", "trait", "true", "try", "type", "typeof", "unsafe",
    "unsized", "use", "virtual", "where", "while", "yield", "abstract",
    "become",
}

errors: list[str] = []


def fail(message: str) -> None:
    errors.append(message)


# ---------------------------------------------------------------------------
# Stage 1: load the header with .inc files inlined
# ---------------------------------------------------------------------------

def load_source() -> str:
    def expand(path: Path) -> str:
        out: list[str] = []
        for line in path.read_text(encoding="utf-8").splitlines():
            match = re.match(r'\s*#\s*include\s+"pinbridge/generated/([\w.]+)"', line)
            if match:
                inc = path.parent / "generated" / match.group(1)
                out.append(expand(inc))
            else:
                out.append(line)
        return "\n".join(out)

    return expand(HEADER)


def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    text = re.sub(r"//[^\n]*", "", text)
    return text


def logical_lines(text: str) -> list[str]:
    """Join backslash continuations, return non-empty stripped lines."""
    joined = re.sub(r"\\\r?\n", " ", text)
    return [line.strip() for line in joined.splitlines() if line.strip()]


# ---------------------------------------------------------------------------
# Stage 2: collect macros, separate them from declaration text
# ---------------------------------------------------------------------------

def collect_macros(lines: list[str]):
    obj_macros: dict[str, str] = {}
    fn_macros: dict[str, tuple[list[str], str]] = {}
    code_lines: list[str] = []
    for line in lines:
        match = re.match(r"#\s*define\s+(\w+)\(([^)]*)\)\s*(.*)$", line)
        if match:
            name, args, body = match.groups()
            fn_macros[name] = ([a.strip() for a in args.split(",") if a.strip()], body)
            continue
        match = re.match(r"#\s*define\s+(\w+)\s*(.*)$", line)
        if match:
            name, value = match.groups()
            obj_macros[name] = value.strip()
            continue
        if line.startswith("#"):
            continue  # include guards, #undef
        if re.fullmatch(r'extern\s+"C"\s*\{', line) or line == "}":
            continue  # extern "C" wrapper
        code_lines.append(line)
    return obj_macros, fn_macros, "\n".join(code_lines)


def expand_object_macros(text: str, obj_macros: dict[str, str]) -> str:
    for _ in range(8):
        changed = False
        for name, value in obj_macros.items():
            if name in KEEP_TOKENS or not value:
                continue
            new = re.sub(rf"\b{re.escape(name)}\b", f" {value} ", text)
            if new != text:
                text, changed = new, True
        if not changed:
            return text
    return text


def expand_function_macros(code: str, fn_macros, obj_macros) -> str:
    """Expand top-level NAME(arg, ...) invocations, then resolve ## pastes and
    object macros so the result is plain C declarations."""
    out = code
    for name, (params, body) in fn_macros.items():
        while True:
            match = re.search(rf"^{re.escape(name)}\((.*)\)\s*$", out, flags=re.M)
            if not match:
                break
            arg_text = match.group(1).strip()
            args = [a.strip() for a in arg_text.split(",")] if arg_text else []
            if len(args) != len(params):
                fail(f"{name}: expected {len(params)} args, got {len(args)}: {arg_text}")
                break
            expanded = body
            for param, arg in zip(params, args):
                # token pasting: PREFIX_##param or param##suffix
                expanded = re.sub(rf"(\w+)##{re.escape(param)}\b", rf"\g<1>{arg}", expanded)
                expanded = re.sub(rf"\b{re.escape(param)}##(\w+)", rf"{arg}\g<1>", expanded)
                expanded = re.sub(rf"\b{re.escape(param)}\b", arg, expanded)
            out = out[: match.start()] + expanded + out[match.end():]
    return expand_object_macros(out, obj_macros)


# ---------------------------------------------------------------------------
# Stage 3: parse declarations
# ---------------------------------------------------------------------------

STRUCT_RE = re.compile(
    r"typedef\s+struct\s+(?P<tag>\w+)?\s*\{(?P<body>.*?)\}\s*(?P<name>\w+)\s*;", re.S)
OPAQUE_RE = re.compile(
    r"typedef\s+(?P<const>const\s+)?struct\s+(?P<tag>\w+)\s*\*\s*(?P<name>\w+)\s*;")
CALLBACK_RE = re.compile(
    r"typedef\s+(?P<ret>[\w\s\*]+?)\(\s*PB_CALL\s*\*\s*(?P<name>\w+)\s*\)\s*\((?P<params>[^)]*)\)\s*;")
SIMPLE_TYPEDEF_RE = re.compile(r"typedef\s+(?P<base>[\w\s]+?)\s+(?P<name>\w+)\s*;")
PROTO_RE = re.compile(
    r"PB_API\s+(?P<noreturn>PB_NORETURN\s+)?(?P<ret>[\w\s\*]+?)\s+PB_CALL\s+"
    r"(?P<name>\w+)\s*\((?P<params>.*?)\)\s*;", re.S)


class Model:
    def __init__(self) -> None:
        self.aliases: dict[str, str] = {}        # PbName -> C base type
        self.structs: dict[str, list[tuple[str, str, str | None]]] = {}
        self.opaques: dict[str, tuple[str, bool]] = {}  # alias -> (tag, is_const)
        self.callbacks: dict[str, tuple[str, list[tuple[str, str]]]] = {}
        self.prototypes: list[tuple[str, str, list[tuple[str, str]], bool]] = []
        self.constants: list[tuple[str, str, object]] = []  # (name, rust type, value)


def parse_decls(code: str, model: Model) -> None:
    for match in STRUCT_RE.finditer(code):
        name, body = match.group("name"), match.group("body")
        fields: list[tuple[str, str, str | None]] = []
        for raw in body.split(";"):
            raw = raw.strip()
            if not raw:
                continue
            field = re.match(r"^([\w\s\*]+?)\s+(\w+)\s*(?:\[\s*([^\]]+)\s*\])?$", raw)
            if not field:
                fail(f"struct {name}: unrecognized field {raw!r}")
                continue
            ftype, fname, arr = field.groups()
            if arr is not None:
                arr = arr.strip()
            fields.append((" ".join(ftype.split()), fname, arr))
        model.structs[name] = fields
    code = STRUCT_RE.sub(" ", code)

    for match in OPAQUE_RE.finditer(code):
        model.opaques[match.group("name")] = (
            match.group("tag"), bool(match.group("const")))
    code = OPAQUE_RE.sub(" ", code)

    for match in CALLBACK_RE.finditer(code):
        ret = " ".join(match.group("ret").split())
        params = parse_params(match.group("params"))
        model.callbacks[match.group("name")] = (ret, params)
    code = CALLBACK_RE.sub(" ", code)

    for match in SIMPLE_TYPEDEF_RE.finditer(code):
        base = " ".join(match.group("base").split())
        model.aliases[match.group("name")] = base
    code = SIMPLE_TYPEDEF_RE.sub(" ", code)

    for match in PROTO_RE.finditer(code):
        ret = " ".join(match.group("ret").split())
        params = parse_params(match.group("params"))
        model.prototypes.append(
            (match.group("name"), ret, params, bool(match.group("noreturn"))))
    code = PROTO_RE.sub(" ", code)

    leftover = code.strip()
    if leftover:
        fail(f"unparsed declaration text remains: {leftover[:200]!r}")


def parse_params(text: str) -> list[tuple[str, str]]:
    text = " ".join(text.split())
    if text in ("", "void"):
        return []
    params: list[tuple[str, str]] = []
    for raw in text.split(","):
        raw = raw.strip()
        match = re.match(r"^(.+?[\*\s])(\w+)$", raw)
        if not match:
            fail(f"unrecognized parameter: {raw!r}")
            continue
        ptype, pname = match.groups()
        params.append((" ".join(ptype.split()), pname))
    return params


# ---------------------------------------------------------------------------
# Stage 4: constants
# ---------------------------------------------------------------------------

INT_SUFFIX_RE = re.compile(r"(?<=\d)[uUlL]+\b")
HEX_RE = re.compile(r"0[xX][0-9a-fA-F]+")
UINT_C_RE = re.compile(r"U?INT(?:8|16|32|64)_C\(([^)]+)\)")


def strip_outer_parens(text: str) -> str:
    while text.startswith("(") and text.endswith(")"):
        depth = 0
        wraps = True
        for index, char in enumerate(text):
            if char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
                if depth == 0 and index != len(text) - 1:
                    wraps = False
                    break
        if not wraps or depth != 0:
            break
        text = text[1:-1].strip()
    return text


def eval_const(name: str, expr: str, values: dict[str, object]):
    """Returns (rust_type, python_value) or None; records failure via fail()."""
    expr = expr.strip()
    if not expr:
        return None
    if expr.startswith('"'):
        return ("&str", expr.strip('"'))
    expr = strip_outer_parens(expr)
    rust_type = "u32"
    cast = re.match(r"^\((Pb\w+)\)\s*(.*)$", expr, flags=re.S)
    if cast:
        rust_type = cast.group(1)
        expr = strip_outer_parens(cast.group(2).strip())
    elif re.match(r"^UINT64_C\(", expr):
        rust_type = "u64"
    elif re.match(r"^INT64_C\(", expr):
        rust_type = "i64"
    if rust_type == "u32" and ("UINT64_MAX" in expr or "INT64_MAX" in expr):
        rust_type = "u64"
    expr = UINT_C_RE.sub(r"\1", expr)
    expr = HEX_RE.sub(lambda m: str(int(m.group(0), 16)), expr)
    expr = expr.replace("UINT32_MAX", str(2**32 - 1)).replace("UINT64_MAX", str(2**64 - 1))
    expr = INT_SUFFIX_RE.sub("", expr)

    def repl(match: re.Match[str]) -> str:
        ident = match.group(0)
        if ident in values:
            return str(values[ident])
        return ident

    expr_eval = re.sub(r"\b[A-Za-z_]\w*\b", repl, expr)
    if re.search(r"[A-Za-z_]", expr_eval):
        fail(f"constant {name}: unresolved identifiers in {expr!r}")
        return None
    try:
        value = eval(expr_eval, {"__builtins__": {}}, {})  # noqa: S307 - ints only
    except Exception as exc:  # pragma: no cover - defensive
        fail(f"constant {name}: cannot evaluate {expr!r}: {exc}")
        return None
    if isinstance(value, bool) or not isinstance(value, int):
        fail(f"constant {name}: non-integer result from {expr!r}")
        return None
    return (rust_type, value)


def collect_constants(obj_macros, model: Model) -> None:
    values: dict[str, object] = {}
    for name, expr in obj_macros.items():
        if name in KEEP_TOKENS or not name.startswith("PB_"):
            continue
        if not expr:
            continue  # PB_FAST_ANALYSIS_CALL and friends
        if "_C_TYPE_" in name or "_C_ARG_" in name or "_C_INPUT_" in name:
            continue  # type macros for the .inc prototype generators
        result = eval_const(name, expr, values)
        if result is None:
            continue
        rust_type, value = result
        if isinstance(value, int):
            values[name] = value
        if rust_type.startswith("Pb"):
            base = model.aliases.get(rust_type)
            bits = 64 if base in ("uint64_t", "int64_t") else 32
            signed = base in ("int32_t", "int64_t") if base else False
            if isinstance(value, int) and value < 0 and not signed:
                value += 2**bits
        model.constants.append((name, rust_type, value))


# ---------------------------------------------------------------------------
# Stage 5: emit Rust
# ---------------------------------------------------------------------------

def rust_type(ctype: str, model: Model, aliases: dict[str, str]) -> str:
    ctype = " ".join(ctype.split())
    segments = [part.strip() for part in ctype.split("*")]
    base = segments[0]
    base_const = False
    if base.startswith("const "):
        base_const = True
        base = base[6:].strip()
    if base.endswith(" const"):
        base = base[:-6].strip()
    if base in BASE_TYPES:
        result = BASE_TYPES[base]
    elif base in aliases or base in model.structs or base in model.callbacks \
            or base in model.opaques:
        result = base
    else:
        fail(f"unmapped C type: {ctype!r}")
        result = "u64 /* UNMAPPED */"
    if result == "()" and len(segments) > 1:
        result = "c_void"
    for index in range(1, len(segments)):
        pointee_const = base_const if index == 1 else "const" in segments[index - 1]
        if result == "c_char":
            result = "*const c_char" if pointee_const else "*mut c_char"
        else:
            result = f"*{'const' if pointee_const else 'mut'} {result}"
    return result


def sanitize(name: str) -> str:
    return f"r#{name}" if name in RUST_KEYWORDS else name


def emit(model: Model) -> str:
    aliases = dict(model.aliases)
    lines: list[str] = []
    emit_ln = lines.append
    emit_ln("// Generated by tools/generate_rust_bindings.py from pinbridge.h. Do not edit.")
    emit_ln("")
    emit_ln("use core::ffi::{c_char, c_void};")
    emit_ln("")

    for name, rust_ty, value in model.constants:
        if rust_ty == "&str":
            emit_ln(f'pub const {name}: &str = "{value}";')
        elif rust_ty in model.opaques:
            _tag, is_const = model.opaques[rust_ty]
            null_fn = "core::ptr::null()" if is_const else "core::ptr::null_mut()"
            if value != 0:
                fail(f"constant {name}: non-zero value for opaque handle type")
            emit_ln(f"pub const {name}: {rust_ty} = {null_fn};")
        else:
            emit_ln(f"pub const {name}: {rust_ty} = {value};")
    emit_ln("")

    for name, base in aliases.items():
        emit_ln(f"pub type {name} = {rust_type(base, model, aliases)};")
    emit_ln("")

    emitted_tags: set[str] = set()
    for alias, (tag, _is_const) in model.opaques.items():
        if tag not in model.structs and tag not in emitted_tags:
            emit_ln(f"pub enum {tag} {{}}")
            emitted_tags.add(tag)
    for alias, (tag, is_const) in model.opaques.items():
        if tag in model.structs:
            pointee = tag
        else:
            pointee = tag
        mut = "*mut" if not is_const else "*const"
        emit_ln(f"pub type {alias} = {mut} {pointee};")
    emit_ln("")

    for name, fields in model.structs.items():
        emit_ln("#[repr(C)]")
        emit_ln("#[derive(Copy, Clone)]")
        emit_ln(f"pub struct {name} {{")
        for ftype, fname, arr in fields:
            rty = rust_type(ftype, model, aliases)
            if arr is not None:
                size = INT_SUFFIX_RE.sub("", arr.strip())
                if not re.fullmatch(r"\d+", size):
                    size = f"{sanitize(size)} as usize"
                emit_ln(f"    pub {sanitize(fname)}: [{rty}; {size}],")
            else:
                emit_ln(f"    pub {sanitize(fname)}: {rty},")
        emit_ln("}")
        emit_ln("")

    for name, (ret, params) in model.callbacks.items():
        rparams = ", ".join(
            f"{sanitize(pname)}: {rust_type(ptype, model, aliases)}" for ptype, pname in params)
        rret = rust_type(ret, model, aliases)
        arrow = "" if rret == "()" else f" -> {rret}"
        emit_ln(f"pub type {name} = Option<unsafe extern \"C\" fn({rparams}){arrow}>;")
    emit_ln("")

    emit_ln('unsafe extern "C" {')
    for name, ret, params, noreturn in model.prototypes:
        rparams = ", ".join(
            f"{sanitize(pname)}: {rust_type(ptype, model, aliases)}" for ptype, pname in params)
        rret = "!" if noreturn else rust_type(ret, model, aliases)
        arrow = "" if rret == "()" else f" -> {rret}"
        emit_ln(f"    pub fn {name}({rparams}){arrow};")
    emit_ln("}")
    emit_ln("")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# main
# ---------------------------------------------------------------------------

def main() -> int:
    source = strip_comments(load_source())
    lines = logical_lines(source)
    obj_macros, fn_macros, code = collect_macros(lines)
    code = expand_function_macros(code, fn_macros, obj_macros)

    model = Model()
    parse_decls(code, model)
    collect_constants(obj_macros, model)

    if errors:
        for message in errors:
            print(f"ERROR: {message}", file=sys.stderr)
        return 1

    names = [name for name, _, _, _ in model.prototypes]
    duplicates = sorted({n for n in names if names.count(n) > 1})
    if duplicates:
        print(f"ERROR: duplicate prototypes: {duplicates}", file=sys.stderr)
        return 1

    OUT_RS.parent.mkdir(parents=True, exist_ok=True)
    rendered = emit(model)
    if errors:
        for message in errors:
            print(f"ERROR: {message}", file=sys.stderr)
        return 1
    OUT_RS.write_text(rendered, encoding="utf-8", newline="\n")
    OUT_EXPORTS.parent.mkdir(parents=True, exist_ok=True)
    OUT_EXPORTS.write_text("\n".join(sorted(names)) + "\n", encoding="utf-8", newline="\n")

    print(f"constants : {len(model.constants)}")
    print(f"aliases   : {len(model.aliases)}")
    print(f"structs   : {len(model.structs)}")
    print(f"opaques   : {len(model.opaques)}")
    print(f"callbacks : {len(model.callbacks)}")
    print(f"functions : {len(model.prototypes)}")
    print(f"wrote {OUT_RS}")
    print(f"wrote {OUT_EXPORTS}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
