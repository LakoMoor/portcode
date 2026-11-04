use serde::{Deserialize, Serialize};

/// Container / executable format guessed from magic bytes. A guess, not proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Elf,
    Pe,
    MachO,
    MachOFat,
    Wasm,
    Zip,
    Gzip,
    Png,
    Jpeg,
    Pdf,
    Sqlite,
    Dex,
    JavaClass,
    Script,
    Text,
    Unknown,
}

impl Format {
    pub fn label(self) -> &'static str {
        match self {
            Format::Elf => "ELF executable/library",
            Format::Pe => "PE (Windows) executable/DLL",
            Format::MachO => "Mach-O",
            Format::MachOFat => "Mach-O universal (fat)",
            Format::Wasm => "WebAssembly module",
            Format::Zip => "ZIP container (jar/apk/docx/…)",
            Format::Gzip => "gzip",
            Format::Png => "PNG image",
            Format::Jpeg => "JPEG image",
            Format::Pdf => "PDF",
            Format::Sqlite => "SQLite database",
            Format::Dex => "Android DEX",
            Format::JavaClass => "Java class",
            Format::Script => "script (shebang)",
            Format::Text => "text",
            Format::Unknown => "unknown binary",
        }
    }
}

/// Classify the first bytes of a file.
pub fn sniff(head: &[u8]) -> Format {
    let starts = |m: &[u8]| head.starts_with(m);
    if starts(b"\x7fELF") {
        Format::Elf
    } else if starts(b"MZ") {
        Format::Pe
    } else if starts(&[0xCF, 0xFA, 0xED, 0xFE])
        || starts(&[0xCE, 0xFA, 0xED, 0xFE])
        || starts(&[0xFE, 0xED, 0xFA, 0xCF])
        || starts(&[0xFE, 0xED, 0xFA, 0xCE])
    {
        Format::MachO
    } else if starts(&[0xCA, 0xFE, 0xBA, 0xBE]) {
        // Java classes share the magic with fat Mach-O; fat headers carry a small arch count.
        match head.get(4..8).and_then(|b| <[u8; 4]>::try_from(b).ok()) {
            Some(n) if u32::from_be_bytes(n) < 32 => Format::MachOFat,
            _ => Format::JavaClass,
        }
    } else if starts(b"\0asm") {
        Format::Wasm
    } else if starts(b"PK\x03\x04") {
        Format::Zip
    } else if starts(&[0x1f, 0x8b]) {
        Format::Gzip
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        Format::Png
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        Format::Jpeg
    } else if starts(b"%PDF-") {
        Format::Pdf
    } else if starts(b"SQLite format 3\0") {
        Format::Sqlite
    } else if starts(b"dex\n") {
        Format::Dex
    } else if starts(b"#!") {
        Format::Script
    } else if !head.is_empty() && std::str::from_utf8(head).is_ok() && !head.contains(&0) {
        Format::Text
    } else {
        Format::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_common_formats() {
        assert_eq!(sniff(b"\x7fELF\x02"), Format::Elf);
        assert_eq!(sniff(b"MZ\x90\0"), Format::Pe);
        assert_eq!(sniff(&[0xCF, 0xFA, 0xED, 0xFE, 7]), Format::MachO);
        assert_eq!(
            sniff(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 2]),
            Format::MachOFat
        );
        assert_eq!(
            sniff(&[0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 52]),
            Format::JavaClass
        );
        assert_eq!(sniff(b"\0asm\x01"), Format::Wasm);
        assert_eq!(sniff(b"fn main() {}"), Format::Text);
        assert_eq!(sniff(&[0, 1, 2, 255]), Format::Unknown);
        assert_eq!(sniff(b""), Format::Unknown);
    }
}
