//! Static-inferred VPD name validation. Unverified against original execution.
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    Empty,
    Unsupported,
    EdgeWhitespace,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty VPD bone name",
            Self::Unsupported => "VPD name contains unsupported characters",
            Self::EdgeWhitespace => "VPD names cannot start or end with whitespace",
        })
    }
}
impl std::error::Error for NameError {}

/// Validate an already decoded UTF-8 name. `allow_empty` is inferred from w2.
/// Binary anchor: 0x100425ca8..0x100426160; see evidence/safe-name-assembly.txt.
/// Caller roles and original execution remain unverified.
pub fn validate_name(name: &str, allow_empty: bool) -> Result<(), NameError> {
    if name.is_empty() && !allow_empty {
        return Err(NameError::Empty);
    }
    if name.len() > 4096
        || name.contains("//")
        || name.chars().any(|c| {
            let n = c as u32;
            n < 0x20 || (0x7f..0xa0).contains(&n) || matches!(c, ';' | '{' | '}')
        })
    {
        return Err(NameError::Unsupported);
    }
    if name.trim() != name {
        return Err(NameError::EdgeWhitespace);
    }
    Ok(())
}
