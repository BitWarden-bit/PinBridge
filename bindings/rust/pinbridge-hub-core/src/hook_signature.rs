//! C-like function prototype parsing and typed Hook value rendering.
//!
//! PE exports do not contain prototypes. A prototype must therefore carry an
//! explicit source and confidence before Hub asks Agent to use it for ABI
//! capture. Unknown typedefs remain visible as unknown instead of being
//! guessed into a misleading scalar type.

use serde_json::{json, Value};

pub const MAX_PARAMETERS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ValueKind {
    Void,
    Signed,
    Unsigned,
    Bool,
    Float32,
    Float64,
    Pointer,
    Utf8Pointer,
    Utf16Pointer,
    Aggregate,
    Unknown,
}

impl ValueKind {
    fn name(self) -> &'static str {
        match self {
            Self::Void => "void",
            Self::Signed => "signed",
            Self::Unsigned => "unsigned",
            Self::Bool => "bool",
            Self::Float32 => "float32",
            Self::Float64 => "float64",
            Self::Pointer => "pointer",
            Self::Utf8Pointer => "utf8_pointer",
            Self::Utf16Pointer => "utf16_pointer",
            Self::Aggregate => "aggregate",
            Self::Unknown => "unknown",
        }
    }

    fn capture_code(self) -> u32 {
        match self {
            Self::Float32 => 1,
            Self::Float64 => 2,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeInfo {
    pub spelling: String,
    pub kind: ValueKind,
    /// Fixed width when it is target-independent. Pointer-sized types use
    /// `pointer_sized` and are resolved from Agent's target architecture.
    pub size: Option<u32>,
    pub pointer_sized: bool,
}

impl TypeInfo {
    pub fn size_for(&self, pointer_width: u32) -> Option<u32> {
        self.pointer_sized.then_some(pointer_width).or(self.size)
    }

    fn to_json(&self, pointer_width: u32) -> Value {
        json!({
            "type": self.spelling,
            "kind": self.kind.name(),
            "size": self.size_for(pointer_width).map(|size| size.to_string()),
            "pointer_sized": self.pointer_sized,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub ty: TypeInfo,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookSignature {
    pub prototype: String,
    pub source: String,
    pub confidence: u32,
    pub function: String,
    pub calling_convention: String,
    pub return_type: TypeInfo,
    pub parameters: Vec<Parameter>,
    pub variadic: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureLayout {
    pub calling_convention: u32,
    pub return_kind: u32,
    pub parameter_count: u32,
    pub float_parameter_mask: u32,
}

impl HookSignature {
    pub fn capture_layout(&self) -> CaptureLayout {
        let mut float_parameter_mask = 0u32;
        for (index, parameter) in self.parameters.iter().enumerate() {
            if parameter.ty.kind.capture_code() != 0 {
                float_parameter_mask |= 1u32 << index;
            }
        }
        CaptureLayout {
            calling_convention: u32::from(self.calling_convention == "fastcall"),
            return_kind: self.return_type.kind.capture_code(),
            parameter_count: self.parameters.len() as u32,
            float_parameter_mask,
        }
    }

    pub fn to_json(&self, pointer_width: u32) -> Value {
        json!({
            "prototype": self.prototype,
            "source": self.source,
            "confidence": self.confidence.to_string(),
            "function": self.function,
            "calling_convention": self.calling_convention,
            "return_type": self.return_type.to_json(pointer_width),
            "parameters": self.parameters.iter().enumerate().map(|(index, parameter)| json!({
                "index": index.to_string(),
                "name": parameter.name,
                "type": parameter.ty.spelling,
                "kind": parameter.ty.kind.name(),
                "size": parameter.ty.size_for(pointer_width).map(|size| size.to_string()),
                "pointer_sized": parameter.ty.pointer_sized,
            })).collect::<Vec<_>>(),
            "variadic": self.variadic,
            "capture_limit": MAX_PARAMETERS.to_string(),
        })
    }

    pub fn typed_arguments(&self, raw: &[Value], pointer_width: u32) -> Vec<Value> {
        self.parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                typed_value(
                    index,
                    &parameter.name,
                    &parameter.ty,
                    raw.get(index),
                    pointer_width,
                )
            })
            .collect()
    }

    pub fn typed_return(&self, raw: Option<&Value>, pointer_width: u32) -> Value {
        typed_value(0, "return", &self.return_type, raw, pointer_width)
    }
}

pub fn parse(prototype: &str, source: &str, confidence: u32) -> Result<HookSignature, String> {
    let prototype = prototype.trim().trim_end_matches(';').trim();
    if prototype.is_empty() || prototype.len() > 2048 {
        return Err("signature must contain 1..2048 bytes".into());
    }
    if !matches!(source, "pdb" | "header" | "manual" | "ai_inferred") {
        return Err("signature_source must be pdb, header, manual, or ai_inferred".into());
    }
    if confidence > 100 {
        return Err("signature_confidence must be 0..100".into());
    }
    let open = prototype
        .find('(')
        .ok_or_else(|| "signature must be a C-like function prototype".to_string())?;
    let close = prototype
        .rfind(')')
        .filter(|close| *close > open)
        .ok_or_else(|| "signature is missing the closing ')'".to_string())?;
    if !prototype[close + 1..].trim().is_empty() {
        return Err("unexpected text after function prototype".into());
    }
    let prefix = prototype[..open].trim();
    let function_start = prefix
        .char_indices()
        .rev()
        .find(|(_, character)| !(character.is_ascii_alphanumeric() || *character == '_'))
        .map(|(index, character)| index + character.len_utf8())
        .unwrap_or(0);
    let function = prefix[function_start..].trim();
    if function.is_empty()
        || !function
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err("signature must contain a plain function name".into());
    }
    let declaration = prefix[..function_start].trim();
    let calling_convention = calling_convention(declaration);
    if calling_convention == "vectorcall" {
        return Err("vectorcall signatures are not supported by Hook capture yet".into());
    }
    let return_spelling = remove_calling_convention(declaration);
    if return_spelling.is_empty() {
        return Err("signature return type is missing".into());
    }
    let return_type = classify_type(&return_spelling);

    let mut parameters = Vec::new();
    let mut variadic = false;
    let arguments = prototype[open + 1..close].trim();
    if !arguments.is_empty() && arguments != "void" {
        for declaration in split_parameters(arguments)? {
            if declaration.trim() == "..." {
                variadic = true;
                continue;
            }
            let (name, spelling) = parameter_parts(&declaration, parameters.len());
            parameters.push(Parameter {
                name,
                ty: classify_type(&spelling),
            });
        }
    }
    if parameters.len() > MAX_PARAMETERS {
        return Err(format!(
            "signature has {} fixed parameters; capture limit is {MAX_PARAMETERS}",
            parameters.len()
        ));
    }
    if calling_convention == "fastcall"
        && parameters
            .iter()
            .take(2)
            .any(|parameter| matches!(parameter.ty.kind, ValueKind::Float32 | ValueKind::Float64))
    {
        return Err(
            "x86 fastcall floating parameters in the first two positions are unsupported".into(),
        );
    }
    Ok(HookSignature {
        prototype: format!("{prototype};"),
        source: source.to_string(),
        confidence,
        function: function.to_string(),
        calling_convention: calling_convention.to_string(),
        return_type,
        parameters,
        variadic,
    })
}

fn calling_convention(declaration: &str) -> &'static str {
    let lower = declaration.to_ascii_lowercase();
    if lower.contains("__fastcall") || lower.split_whitespace().any(|word| word == "fastcall") {
        "fastcall"
    } else if lower.contains("__stdcall")
        || lower.split_whitespace().any(|word| {
            matches!(
                word,
                "winapi" | "ntapi" | "callback" | "apientry" | "stdcall"
            )
        })
    {
        "stdcall"
    } else if lower.contains("__vectorcall") {
        "vectorcall"
    } else {
        "cdecl"
    }
}

fn remove_calling_convention(declaration: &str) -> String {
    declaration
        .split_whitespace()
        .filter(|word| {
            !matches!(
                word.to_ascii_lowercase().as_str(),
                "__cdecl"
                    | "cdecl"
                    | "__stdcall"
                    | "stdcall"
                    | "__fastcall"
                    | "fastcall"
                    | "__vectorcall"
                    | "vectorcall"
                    | "winapi"
                    | "ntapi"
                    | "callback"
                    | "apientry"
                    | "extern"
                    | "\"c\""
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_parameters(arguments: &str) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (index, character) in arguments.char_indices() {
        match character {
            '(' | '[' | '<' => depth += 1,
            ')' | ']' | '>' => {
                depth -= 1;
                if depth < 0 {
                    return Err("unbalanced parameter type".into());
                }
            }
            ',' if depth == 0 => {
                result.push(arguments[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err("unbalanced parameter type".into());
    }
    result.push(arguments[start..].trim().to_string());
    if result.iter().any(|parameter| parameter.is_empty()) {
        return Err("empty parameter declaration".into());
    }
    Ok(result)
}

fn parameter_parts(declaration: &str, index: usize) -> (String, String) {
    let declaration = declaration.split('=').next().unwrap_or(declaration).trim();
    if let Some(pointer_open) = declaration.find("(*") {
        let name_start = pointer_open + 2;
        if let Some(name_end) = declaration[name_start..].find(')') {
            let name_end = name_start + name_end;
            let name = declaration[name_start..name_end].trim().to_string();
            let spelling = format!("{} *", declaration[..pointer_open].trim());
            return (name, spelling);
        }
    }
    let before_array = declaration.split('[').next().unwrap_or(declaration).trim();
    let name_start = before_array
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_ascii_alphanumeric() || *character == '_')
        .last()
        .map(|(position, _)| position);
    let Some(name_start) = name_start else {
        return (format!("arg{index}"), declaration.to_string());
    };
    let candidate = before_array[name_start..].trim();
    let type_part = before_array[..name_start].trim();
    if type_part.is_empty() || is_type_keyword(candidate) {
        return (format!("arg{index}"), declaration.to_string());
    }
    let array_suffix = declaration
        .find('[')
        .map(|position| declaration[position..].trim())
        .unwrap_or("");
    let spelling = if array_suffix.is_empty() {
        type_part.to_string()
    } else {
        format!("{type_part} {array_suffix}")
    };
    (candidate.to_string(), spelling)
}

fn is_type_keyword(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "void"
            | "char"
            | "short"
            | "int"
            | "long"
            | "float"
            | "double"
            | "signed"
            | "unsigned"
            | "bool"
            | "size_t"
            | "ssize_t"
    )
}

fn classify_type(spelling: &str) -> TypeInfo {
    let spelling = spelling.trim().replace("  ", " ");
    let lower = spelling.to_ascii_lowercase();
    let compact = lower
        .replace("const", "")
        .replace("volatile", "")
        .replace(' ', "");
    let pointer = compact.contains('*') || pointer_alias(&compact);
    let (kind, size, pointer_sized) = if compact == "void" {
        (ValueKind::Void, Some(0), false)
    } else if pointer {
        let kind = if utf16_pointer(&compact) {
            ValueKind::Utf16Pointer
        } else if utf8_pointer(&compact) {
            ValueKind::Utf8Pointer
        } else {
            ValueKind::Pointer
        };
        (kind, None, true)
    } else if compact == "float" || compact == "f32" {
        (ValueKind::Float32, Some(4), false)
    } else if compact == "double" || compact == "f64" {
        (ValueKind::Float64, Some(8), false)
    } else if matches!(compact.as_str(), "bool" | "boolean" | "bool32" | "winbool") {
        (ValueKind::Bool, Some(4), false)
    } else if matches!(compact.as_str(), "bool8" | "_bool") {
        (ValueKind::Bool, Some(1), false)
    } else if matches!(
        compact.as_str(),
        "size_t" | "uintptr_t" | "ulong_ptr" | "dword_ptr"
    ) {
        (ValueKind::Unsigned, None, true)
    } else if matches!(compact.as_str(), "ssize_t" | "intptr_t" | "long_ptr") {
        (ValueKind::Signed, None, true)
    } else if matches!(
        compact.as_str(),
        "char" | "signedchar" | "int8_t" | "i8" | "c_char" | "__int8" | "signed__int8"
    ) {
        (ValueKind::Signed, Some(1), false)
    } else if matches!(
        compact.as_str(),
        "unsignedchar" | "uint8_t" | "u8" | "byte" | "uchar" | "unsigned__int8"
    ) {
        (ValueKind::Unsigned, Some(1), false)
    } else if matches!(
        compact.as_str(),
        "short" | "shortint" | "signedshort" | "int16_t" | "i16" | "__int16" | "signed__int16"
    ) {
        (ValueKind::Signed, Some(2), false)
    } else if matches!(
        compact.as_str(),
        "unsignedshort"
            | "uint16_t"
            | "u16"
            | "word"
            | "ushort"
            | "wchar_t"
            | "wchar"
            | "unsigned__int16"
    ) {
        (ValueKind::Unsigned, Some(2), false)
    } else if matches!(
        compact.as_str(),
        "int"
            | "signed"
            | "signedint"
            | "long"
            | "signedlong"
            | "int32_t"
            | "i32"
            | "long32"
            | "hresult"
            | "ntstatus"
            | "__int32"
            | "signed__int32"
    ) || compact.starts_with("enum")
    {
        (ValueKind::Signed, Some(4), false)
    } else if matches!(
        compact.as_str(),
        "unsigned"
            | "unsignedint"
            | "unsignedlong"
            | "uint32_t"
            | "u32"
            | "dword"
            | "ulong"
            | "uint"
            | "unsigned__int32"
    ) {
        (ValueKind::Unsigned, Some(4), false)
    } else if matches!(
        compact.as_str(),
        "longlong"
            | "signedlonglong"
            | "int64_t"
            | "i64"
            | "long64"
            | "large_integer"
            | "__int64"
            | "signed__int64"
    ) {
        (ValueKind::Signed, Some(8), false)
    } else if matches!(
        compact.as_str(),
        "unsignedlonglong"
            | "uint64_t"
            | "u64"
            | "ulonglong"
            | "dword64"
            | "ularge_integer"
            | "unsigned__int64"
    ) {
        (ValueKind::Unsigned, Some(8), false)
    } else if matches!(compact.as_str(), "guid" | "uuid") {
        (ValueKind::Aggregate, Some(16), false)
    } else if let Some(size) = compact
        .strip_prefix("struct")
        .or_else(|| compact.strip_prefix("bytes"))
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|size| *size > 0 && *size <= 4096)
    {
        (ValueKind::Aggregate, Some(size), false)
    } else {
        (ValueKind::Unknown, None, false)
    };
    TypeInfo {
        spelling,
        kind,
        size,
        pointer_sized,
    }
}

fn pointer_alias(compact: &str) -> bool {
    matches!(
        compact,
        "handle"
            | "hwnd"
            | "hmodule"
            | "hinstance"
            | "hkey"
            | "hfile"
            | "hprocess"
            | "hthread"
            | "socket"
            | "lpvoid"
            | "lpcvoid"
            | "pvoid"
            | "farproc"
    ) || compact.starts_with("lp")
        || compact.starts_with("pc")
        || compact.starts_with("p_")
}

fn utf8_pointer(compact: &str) -> bool {
    compact.contains("char*") || matches!(compact, "lpstr" | "lpcstr" | "pstr" | "pcstr" | "pchar")
}

fn utf16_pointer(compact: &str) -> bool {
    compact.contains("wchar_t*")
        || compact.contains("wchar*")
        || matches!(
            compact,
            "lpwstr" | "lpcwstr" | "pwstr" | "pcwstr" | "pwchar"
        )
}

fn raw_u64(value: Option<&Value>) -> Option<u64> {
    let value = value?.as_str()?.trim();
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map(|hex| u64::from_str_radix(hex, 16).ok())
        .unwrap_or_else(|| value.parse().ok())
}

fn typed_value(
    index: usize,
    name: &str,
    ty: &TypeInfo,
    raw: Option<&Value>,
    pointer_width: u32,
) -> Value {
    let raw_number = raw_u64(raw);
    let size = ty.size_for(pointer_width);
    let masked = raw_number.map(|value| mask_width(value, size.unwrap_or(pointer_width)));
    let (value, display, quality) = match (ty.kind, masked) {
        (ValueKind::Void, _) => (Value::Null, "void".to_string(), "exact"),
        (_, None) => (Value::Null, "未捕获".to_string(), "missing"),
        (ValueKind::Signed, Some(value)) => {
            let signed = sign_extend(value, size.unwrap_or(pointer_width));
            (
                Value::String(signed.to_string()),
                signed.to_string(),
                "exact",
            )
        }
        (ValueKind::Unsigned, Some(value)) => (
            Value::String(value.to_string()),
            format!("{value} (0x{value:x})"),
            "exact",
        ),
        (ValueKind::Bool, Some(value)) => (
            Value::Bool(value != 0),
            if value == 0 { "false" } else { "true" }.to_string(),
            "exact",
        ),
        (ValueKind::Float32, Some(value)) => {
            let float = f32::from_bits(value as u32);
            let display = format!("{float:?}");
            (Value::String(display.clone()), display, "exact")
        }
        (ValueKind::Float64, Some(value)) => {
            let float = f64::from_bits(value);
            let display = format!("{float:?}");
            (Value::String(display.clone()), display, "exact")
        }
        (ValueKind::Pointer | ValueKind::Utf8Pointer | ValueKind::Utf16Pointer, Some(value)) => {
            let display = if value == 0 {
                "NULL".into()
            } else {
                format!("0x{value:x}")
            };
            (
                Value::String(format!("0x{value:x}")),
                display,
                "address_only",
            )
        }
        (ValueKind::Aggregate, Some(value)) => (
            Value::String(format!("0x{value:x}")),
            format!("ABI槽 0x{value:x}"),
            "aggregate_layout_required",
        ),
        (ValueKind::Unknown, Some(value)) => (
            Value::String(format!("0x{value:x}")),
            format!("0x{value:x}"),
            "unknown_type",
        ),
    };
    json!({
        "index": index.to_string(),
        "name": name,
        "type": ty.spelling,
        "kind": ty.kind.name(),
        "size": size.map(|size| size.to_string()),
        "raw": raw.cloned(),
        "value": value,
        "display": display,
        "quality": quality,
    })
}

fn mask_width(value: u64, size: u32) -> u64 {
    match size {
        0 => 0,
        1..=7 => value & ((1u64 << (size * 8)) - 1),
        _ => value,
    }
}

fn sign_extend(value: u64, size: u32) -> i64 {
    let bits = size.clamp(1, 8) * 8;
    if bits == 64 {
        value as i64
    } else {
        let shift = 64 - bits;
        ((value << shift) as i64) >> shift
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_style_signature_and_layout() {
        let signature = parse(
            "BOOL WINAPI DemoApi(LPCWSTR name, DWORD count, float ratio, double scale)",
            "header",
            100,
        )
        .unwrap();
        assert_eq!(signature.function, "DemoApi");
        assert_eq!(signature.calling_convention, "stdcall");
        assert_eq!(signature.parameters[0].ty.kind, ValueKind::Utf16Pointer);
        assert_eq!(signature.parameters[1].ty.size, Some(4));
        assert_eq!(signature.capture_layout().float_parameter_mask, 0b1100);
    }

    #[test]
    fn signed_width_and_float_bits_are_rendered_from_signature() {
        let signature = parse("double DemoApi(int8_t delta, float ratio)", "manual", 90).unwrap();
        let raw = vec![json!("0xff"), json!("0x3fc00000")];
        let values = signature.typed_arguments(&raw, 8);
        assert_eq!(values[0]["display"], "-1");
        assert_eq!(values[1]["display"], "1.5");
        assert_eq!(signature.capture_layout().return_kind, 2);
    }

    #[test]
    fn rejects_unproven_source_and_more_than_capture_limit() {
        assert!(parse("int Demo(int value)", "guess", 50).is_err());
        assert!(parse("int __vectorcall Demo(float value)", "header", 100).is_err());
        let parameters = (0..17)
            .map(|index| format!("int a{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        assert!(parse(&format!("int Demo({parameters})"), "pdb", 100).is_err());
    }

    #[test]
    fn classifies_msvc_integer_widths_without_guessing() {
        let signature = parse(
            "unsigned __int64 Demo(__int8 small, unsigned __int32 count)",
            "header",
            100,
        )
        .unwrap();
        assert_eq!(signature.return_type.kind, ValueKind::Unsigned);
        assert_eq!(signature.return_type.size, Some(8));
        assert_eq!(signature.parameters[0].ty.size, Some(1));
        assert_eq!(signature.parameters[1].ty.size, Some(4));
    }
}
