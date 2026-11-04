use crate::{sniff, Format};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum PinError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{0} is not a regular file")]
    NotAFile(String),
}

/// Identity of the exact original build a port is measured against. Pinning proves *which* file, nothing more.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pin {
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub format: Format,
    pub version: Option<String>,
    pub pinned: u64,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Stream a file through sha256 (constant memory) and return the digest, size and first 64 bytes.
pub fn sha256_file(path: &Path) -> Result<(String, u64, Vec<u8>), PinError> {
    let io = |source| PinError::Io {
        path: path.display().to_string(),
        source,
    };
    let meta = std::fs::metadata(path).map_err(io)?;
    if !meta.is_file() {
        return Err(PinError::NotAFile(path.display().to_string()));
    }
    let mut f = std::fs::File::open(path).map_err(io)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut head = Vec::new();
    let mut total = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        let chunk = buf.get(..n).unwrap_or(&[]);
        if head.len() < 64 {
            head.extend_from_slice(chunk.get(..chunk.len().min(64 - head.len())).unwrap_or(&[]));
        }
        hasher.update(chunk);
        total = total.saturating_add(n as u64);
    }
    Ok((hex::encode(hasher.finalize()), total, head))
}

impl Pin {
    pub fn of(path: &Path, version: Option<String>) -> Result<Self, PinError> {
        let (sha256, size, head) = sha256_file(path)?;
        Ok(Pin {
            path: path.display().to_string(),
            sha256,
            size,
            format: sniff(&head),
            version,
            pinned: crate::now_secs(),
        })
    }

    /// Does the file on disk still match this pin?
    pub fn verify(&self) -> Result<bool, PinError> {
        Ok(sha256_file(Path::new(&self.path))?.0 == self.sha256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pin_detects_change() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.bin");
        std::fs::write(&p, b"\x7fELFhello").unwrap();
        let pin = Pin::of(&p, Some("1.0".into())).unwrap();
        assert_eq!(pin.format, Format::Elf);
        assert!(pin.verify().unwrap());
        std::fs::write(&p, b"\x7fELFhellO").unwrap();
        assert!(!pin.verify().unwrap());
    }
    #[test]
    fn sha_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn directory_is_not_a_file() {
        let d = tempfile::tempdir().unwrap();
        assert!(matches!(
            Pin::of(d.path(), None),
            Err(PinError::NotAFile(_))
        ));
    }
}
