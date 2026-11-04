//! L0 — evidence records, the append-only ledger, file pinning and format sniffing.
//!
//! Everything PortCode learns about an original program is stored as an [`Evidence`] record that is
//! honest about *how* it is known (observation / inference / unknown) and *how sure* we are.
//! No I/O beyond the ledger/pin files lives here; no network, no process spawning.
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing
    )
)]

mod ledger;
mod pin;
mod sniff;

pub use ledger::{Confidence, Evidence, Kind, Ledger, LedgerError, Source};
pub use pin::{sha256_file, sha256_hex, Pin, PinError};
pub use sniff::{sniff, Format};

/// Seconds since the Unix epoch (0 if the clock is before it).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
