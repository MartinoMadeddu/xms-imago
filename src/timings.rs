//! How long the steps of loading and showing a scene take, printed when
//! `XMS_TIMINGS=1` is set: `XMS_TIMINGS=1 cargo run --release`. For finding
//! where a big stage (Moana) spends its time, not for everyday use.
//!
//! ```ignore
//! let _t = crate::timings::span("usd: open and compose");
//! ```
//! prints `[xms] usd: open and compose: 1234 ms` when `_t` goes out of scope.
//! Steps under 20 ms are not printed, so what runs every frame stays quiet
//! until it is slow.

use std::sync::OnceLock;
use std::time::Instant;

/// Whether timings are printed: `XMS_TIMINGS` set to anything but 0.
pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("XMS_TIMINGS").is_ok_and(|v| v != "0" && !v.is_empty()))
}

/// A step being timed: printed when dropped, if timings are on.
pub struct Span {
    label: String,
    start: Option<Instant>,
}

/// Start timing a step.
pub fn span(label: impl Into<String>) -> Span {
    if on() { Span { label: label.into(), start: Some(Instant::now()) } } else { Span { label: String::new(), start: None } }
}

impl Span {
    /// Add what was found to the line printed (counts, sizes).
    pub fn note(&mut self, what: impl std::fmt::Display) {
        if self.start.is_some() { self.label = format!("{} ({what})", self.label); }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.start.filter(|s| s.elapsed().as_millis() >= 20) {
            eprintln!("[xms] {}: {:.0} ms", self.label, start.elapsed().as_secs_f64() * 1000.0);
        }
    }
}
