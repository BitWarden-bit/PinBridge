//! Target-architecture detection and selection for the launcher.
//!
//! The only sanctioned way to pick the Pin runtime is to read the target's
//! PE headers — the `Machine` field of the COFF file header together with the
//! optional-header `Magic` (PE32 vs PE32+). Nothing here looks at the file
//! name, and an unknown or inconsistent PE is a hard error, never a guess.

use std::fmt;
use std::path::Path;

const IMAGE_FILE_MACHINE_I386: u16 = 0x014c;
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;

/// PE optional-header magic: PE32 (32-bit) vs PE32+ (64-bit).
pub const OPTIONAL_MAGIC_PE32: u16 = 0x010b;
pub const OPTIONAL_MAGIC_PE32PLUS: u16 = 0x020b;
pub const IMAGE_SUBSYSTEM_WINDOWS_GUI: u16 = 2;
pub const IMAGE_SUBSYSTEM_WINDOWS_CUI: u16 = 3;

const DOS_MAGIC: &[u8; 2] = b"MZ";
const NT_SIGNATURE: &[u8; 4] = b"PE\0\0";

/// The architecture a target (or an explicitly requested override) selects.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Arch {
    X86,
    X64,
}

impl Arch {
    pub fn as_str(self) -> &'static str {
        match self {
            Arch::X86 => "x86",
            Arch::X64 => "x64",
        }
    }

    /// Pin kit directory name: `ia32` for 32-bit, `intel64` for 64-bit.
    pub fn runtime_dir(self) -> &'static str {
        match self {
            Arch::X86 => "ia32",
            Arch::X64 => "intel64",
        }
    }

    pub fn pointer_width(self) -> u32 {
        match self {
            Arch::X86 => 4,
            Arch::X64 => 8,
        }
    }

    /// COFF `Machine` value this architecture must declare.
    pub fn machine_id(self) -> u16 {
        match self {
            Arch::X86 => IMAGE_FILE_MACHINE_I386,
            Arch::X64 => IMAGE_FILE_MACHINE_AMD64,
        }
    }

    /// Optional-header `Magic` value this architecture must declare.
    pub fn optional_magic(self) -> u16 {
        match self {
            Arch::X86 => OPTIONAL_MAGIC_PE32,
            Arch::X64 => OPTIONAL_MAGIC_PE32PLUS,
        }
    }

    /// Wire id from `pinbridge_proto::ARCH_*`.
    pub fn wire_id(self) -> u32 {
        match self {
            Arch::X86 => pinbridge_proto::ARCH_X86,
            Arch::X64 => pinbridge_proto::ARCH_X64,
        }
    }

    /// Parses a `--arch` flag value. Accepts the canonical spellings plus the
    /// common aliases; anything else is an error, never a silent default.
    pub fn parse(text: &str) -> Result<Arch, String> {
        match text.to_ascii_lowercase().as_str() {
            "x86" | "ia32" | "32" => Ok(Arch::X86),
            "x64" | "intel64" | "64" | "amd64" => Ok(Arch::X64),
            other => Err(format!(
                "unknown architecture {other:?} (expected auto, x86 or x64)"
            )),
        }
    }
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Structured PE identification result: the two fields that decide the
/// architecture, plus the resolved [`Arch`].
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct PeInfo {
    pub machine: u16,
    pub optional_magic: u16,
    pub subsystem: u16,
    pub arch: Arch,
}

/// Reads the DOS + COFF + optional headers of `path` and resolves the target
/// architecture. Returns a descriptive error when the file is not a PE, or
/// when its machine/magic pair is unknown or inconsistent.
pub fn detect_pe_arch(path: &Path) -> Result<Arch, String> {
    let data = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    parse_pe(&data).map(|info| info.arch)
}

/// Reads the target's PE subsystem. Launchers use this to give console
/// targets a visible console when their parent is a GUI application.
pub fn detect_pe_subsystem(path: &Path) -> Result<u16, String> {
    let data = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    parse_pe(&data).map(|info| info.subsystem)
}

/// Pure PE-header parser over an in-memory image (unit-testable without a
/// file on disk). Uses explicit little-endian offsets — no `transmute`, so it
/// is alignment-safe and portable.
pub fn parse_pe(data: &[u8]) -> Result<PeInfo, String> {
    if data.len() < 0x40 {
        return Err("file too short for a DOS header".to_string());
    }
    if &data[0..2] != DOS_MAGIC {
        return Err("not a PE image (missing MZ magic)".to_string());
    }
    let pe_offset = read_u32(data, 0x3c)? as usize;
    if pe_offset + 24 > data.len() {
        return Err("PE header offset out of range".to_string());
    }
    if &data[pe_offset..pe_offset + 4] != NT_SIGNATURE {
        return Err("not a PE image (missing PE\\0\\0 signature)".to_string());
    }

    // COFF file header sits right after the 4-byte signature.
    let file_header = pe_offset + 4;
    let machine = read_u16(data, file_header)?;
    let size_of_optional = read_u16(data, file_header + 16)? as usize;
    if size_of_optional < 70 {
        return Err("PE optional header is too short for the subsystem field".to_string());
    }
    let optional_header = file_header + 20;
    if optional_header + size_of_optional > data.len() {
        return Err("PE optional header out of range".to_string());
    }
    let optional_magic = read_u16(data, optional_header)?;
    let subsystem = read_u16(data, optional_header + 68)?;

    let arch = match (machine, optional_magic) {
        (IMAGE_FILE_MACHINE_I386, OPTIONAL_MAGIC_PE32) => Arch::X86,
        (IMAGE_FILE_MACHINE_AMD64, OPTIONAL_MAGIC_PE32PLUS) => Arch::X64,
        (machine, magic) => {
            return Err(format!(
                "unsupported or inconsistent PE: machine 0x{machine:04x}, optional magic 0x{magic:04x} \
                 (expected I386+PE32 (0x010b) or AMD64+PE32+ (0x020b))"
            ));
        }
    };
    Ok(PeInfo {
        machine,
        optional_magic,
        subsystem,
        arch,
    })
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, String> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| "PE header truncated".to_string())?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| "PE header truncated".to_string())?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal, structurally valid PE image builder for tests: lays out the
    /// DOS header, the NT signature, a COFF file header and an optional
    /// header with the requested machine/magic pair.
    fn pe_image(machine: u16, optional_magic: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 0x200];
        buf[0..2].copy_from_slice(b"MZ");
        // e_lfanew at 0x3c -> point at 0x80, leaving a small DOS stub.
        let pe_offset = 0x80usize;
        buf[0x3c..0x40].copy_from_slice(&(pe_offset as u32).to_le_bytes());
        buf[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
        let file_header = pe_offset + 4;
        buf[file_header..file_header + 2].copy_from_slice(&machine.to_le_bytes());
        buf[file_header + 16..file_header + 18].copy_from_slice(&0xE0u16.to_le_bytes()); // size of optional
        let optional_header = file_header + 20;
        buf[optional_header..optional_header + 2].copy_from_slice(&optional_magic.to_le_bytes());
        buf[optional_header + 68..optional_header + 70]
            .copy_from_slice(&IMAGE_SUBSYSTEM_WINDOWS_CUI.to_le_bytes());
        buf
    }

    #[test]
    fn detects_pe32_as_x86() {
        let info = parse_pe(&pe_image(IMAGE_FILE_MACHINE_I386, OPTIONAL_MAGIC_PE32)).unwrap();
        assert_eq!(info.arch, Arch::X86);
        assert_eq!(info.machine, IMAGE_FILE_MACHINE_I386);
        assert_eq!(info.optional_magic, OPTIONAL_MAGIC_PE32);
        assert_eq!(info.subsystem, IMAGE_SUBSYSTEM_WINDOWS_CUI);
    }

    #[test]
    fn detects_pe32plus_as_x64() {
        let info = parse_pe(&pe_image(IMAGE_FILE_MACHINE_AMD64, OPTIONAL_MAGIC_PE32PLUS)).unwrap();
        assert_eq!(info.arch, Arch::X64);
        assert_eq!(info.subsystem, IMAGE_SUBSYSTEM_WINDOWS_CUI);
    }

    #[test]
    fn rejects_inconsistent_machine_magic() {
        // I386 machine but PE32+ magic is not a valid PE — must error, not guess.
        let err =
            parse_pe(&pe_image(IMAGE_FILE_MACHINE_I386, OPTIONAL_MAGIC_PE32PLUS)).unwrap_err();
        assert!(err.contains("inconsistent"), "unexpected error: {err}");
        let err = parse_pe(&pe_image(IMAGE_FILE_MACHINE_AMD64, OPTIONAL_MAGIC_PE32)).unwrap_err();
        assert!(err.contains("inconsistent"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_unknown_machine() {
        // ARM64 (0xaa64) + PE32+ is a real PE but not supported here.
        let err = parse_pe(&pe_image(0xaa64, OPTIONAL_MAGIC_PE32PLUS)).unwrap_err();
        assert!(err.contains("unsupported"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_non_pe() {
        // Long enough (0x40 bytes) to pass the DOS-header length check, so the
        // parser reaches the magic check and must reject the non-"MZ" signature.
        let mut buf = [0u8; 0x40];
        buf[0] = b'n';
        buf[1] = b'o';
        let err = parse_pe(&buf).unwrap_err();
        assert!(err.contains("MZ"), "unexpected error: {err}");
    }

    #[test]
    fn rejects_truncated() {
        assert!(parse_pe(&[0u8; 16]).is_err());
        assert!(parse_pe(&pe_image(IMAGE_FILE_MACHINE_I386, OPTIONAL_MAGIC_PE32)[..0x40]).is_err());
    }

    #[test]
    fn arch_flag_parsing() {
        assert_eq!(Arch::parse("x86").unwrap(), Arch::X86);
        assert_eq!(Arch::parse("ia32").unwrap(), Arch::X86);
        assert_eq!(Arch::parse("x64").unwrap(), Arch::X64);
        assert_eq!(Arch::parse("intel64").unwrap(), Arch::X64);
        assert_eq!(Arch::parse("amd64").unwrap(), Arch::X64);
        assert!(Arch::parse("auto").is_err());
        assert!(Arch::parse("arm64").is_err());
    }

    #[test]
    fn arch_layout_matches_pin_kit() {
        assert_eq!(Arch::X86.runtime_dir(), "ia32");
        assert_eq!(Arch::X64.runtime_dir(), "intel64");
        assert_eq!(Arch::X86.pointer_width(), 4);
        assert_eq!(Arch::X64.pointer_width(), 8);
    }

    #[test]
    fn detects_real_fixture_when_present() {
        // If the 32-bit fixture has been compiled (fixtures/x86/build.ps1),
        // exercise detection against a real PE. Absent fixture is a structured
        // skip, not a failure — the synthesized images above already cover the
        // parser, so a machine without an x86 compiler loses nothing.
        let exe = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("..")
            .join("fixtures")
            .join("x86")
            .join("hello32.exe");
        if !exe.exists() {
            return;
        }
        let info = parse_pe(&std::fs::read(&exe).unwrap()).unwrap();
        // The exact fields the fixture must declare: I386 machine + PE32 magic.
        assert_eq!(info.machine, IMAGE_FILE_MACHINE_I386);
        assert_eq!(info.optional_magic, OPTIONAL_MAGIC_PE32);
        assert_eq!(info.arch, Arch::X86);
    }
}
