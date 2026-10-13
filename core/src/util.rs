//! Small helpers with no external dependencies.

use std::time::{SystemTime, UNIX_EPOCH};

/// A compact, filesystem-safe, monotonic-ish timestamp for naming backup
/// folders. Seconds since the Unix epoch — unique enough per run and needs no
/// date/time crate.
pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
