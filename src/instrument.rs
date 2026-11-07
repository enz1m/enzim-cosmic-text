// Modified by enzUI contributors. See FORK.md.
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Deterministic counters for text-buffer dirty discovery and shaping.
//!
//! These counters are intended for regression tests and diagnostics. They are
//! thread-local when the `instrumentation` feature is enabled, so independent
//! editor tests do not charge work to each other. Without that feature the API
//! remains available and reports zero at no runtime cost.

/// A snapshot of buffer work performed on the calling thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    /// Calls to the conservative public shaping entry point.
    pub shape_calls: u64,
    /// Whole-buffer dirty-discovery passes started.
    pub dirty_scan_calls: u64,
    /// Logical lines inspected by dirty discovery.
    pub dirty_lines_inspected: u64,
    /// Existing line layouts invalidated by a buffer-wide property change.
    pub layouts_invalidated: u64,
    /// Logical lines whose shape or layout cache was actually rebuilt.
    pub lines_shaped: u64,
    /// Visible-window shaping passes that dirty resolution actually ran.
    ///
    /// `shape_until_scroll` is called on every frame, but it returns early
    /// when the buffer is clean. This counts the passes that were *not*
    /// skipped, so a caller can prove that an idle frame announced no
    /// mutation and therefore walked no visible rows (enzUI-ICED #1802).
    pub visible_shape_passes: u64,
}

impl Counters {
    /// Returns work recorded since `earlier`.
    pub fn since(self, earlier: Self) -> Self {
        Self {
            shape_calls: self.shape_calls.saturating_sub(earlier.shape_calls),
            dirty_scan_calls: self
                .dirty_scan_calls
                .saturating_sub(earlier.dirty_scan_calls),
            dirty_lines_inspected: self
                .dirty_lines_inspected
                .saturating_sub(earlier.dirty_lines_inspected),
            layouts_invalidated: self
                .layouts_invalidated
                .saturating_sub(earlier.layouts_invalidated),
            lines_shaped: self.lines_shaped.saturating_sub(earlier.lines_shaped),
            visible_shape_passes: self
                .visible_shape_passes
                .saturating_sub(earlier.visible_shape_passes),
        }
    }
}

#[cfg(feature = "instrumentation")]
mod enabled {
    use super::Counters;
    use std::cell::Cell;

    std::thread_local! {
        static SHAPE_CALLS: Cell<u64> = const { Cell::new(0) };
        static DIRTY_SCAN_CALLS: Cell<u64> = const { Cell::new(0) };
        static DIRTY_LINES_INSPECTED: Cell<u64> = const { Cell::new(0) };
        static LAYOUTS_INVALIDATED: Cell<u64> = const { Cell::new(0) };
        static LINES_SHAPED: Cell<u64> = const { Cell::new(0) };
        static VISIBLE_SHAPE_PASSES: Cell<u64> = const { Cell::new(0) };
    }

    pub fn counters() -> Counters {
        Counters {
            shape_calls: SHAPE_CALLS.get(),
            dirty_scan_calls: DIRTY_SCAN_CALLS.get(),
            dirty_lines_inspected: DIRTY_LINES_INSPECTED.get(),
            layouts_invalidated: LAYOUTS_INVALIDATED.get(),
            lines_shaped: LINES_SHAPED.get(),
            visible_shape_passes: VISIBLE_SHAPE_PASSES.get(),
        }
    }

    pub fn reset() {
        SHAPE_CALLS.set(0);
        DIRTY_SCAN_CALLS.set(0);
        DIRTY_LINES_INSPECTED.set(0);
        LAYOUTS_INVALIDATED.set(0);
        LINES_SHAPED.set(0);
        VISIBLE_SHAPE_PASSES.set(0);
    }

    fn add(counter: &'static std::thread::LocalKey<Cell<u64>>, amount: usize) {
        counter.set(counter.get().saturating_add(amount as u64));
    }

    pub(crate) fn shape_call() {
        add(&SHAPE_CALLS, 1);
    }

    pub(crate) fn dirty_scan(lines: usize) {
        add(&DIRTY_SCAN_CALLS, 1);
        add(&DIRTY_LINES_INSPECTED, lines);
    }

    pub(crate) fn layouts_invalidated(lines: usize) {
        add(&LAYOUTS_INVALIDATED, lines);
    }

    pub(crate) fn line_shaped() {
        add(&LINES_SHAPED, 1);
    }

    pub(crate) fn visible_shape_pass() {
        add(&VISIBLE_SHAPE_PASSES, 1);
    }
}

#[cfg(feature = "instrumentation")]
pub use enabled::{counters, reset};

#[cfg(feature = "instrumentation")]
pub(crate) use enabled::{
    dirty_scan, layouts_invalidated, line_shaped, shape_call, visible_shape_pass,
};

#[cfg(not(feature = "instrumentation"))]
mod disabled {
    use super::Counters;

    pub const fn counters() -> Counters {
        Counters {
            shape_calls: 0,
            dirty_scan_calls: 0,
            dirty_lines_inspected: 0,
            layouts_invalidated: 0,
            lines_shaped: 0,
            visible_shape_passes: 0,
        }
    }

    pub const fn reset() {}
    pub(crate) const fn shape_call() {}
    pub(crate) const fn dirty_scan(_lines: usize) {}
    pub(crate) const fn layouts_invalidated(_lines: usize) {}
    pub(crate) const fn line_shaped() {}
    pub(crate) const fn visible_shape_pass() {}
}

#[cfg(not(feature = "instrumentation"))]
pub use disabled::{counters, reset};

#[cfg(not(feature = "instrumentation"))]
pub(crate) use disabled::{
    dirty_scan, layouts_invalidated, line_shaped, shape_call, visible_shape_pass,
};
