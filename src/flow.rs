// Modified by enzUI contributors. See FORK.md.
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Text flowing around exclusion rectangles.
//!
//! A [`Buffer`](crate::Buffer) with exclusions lays each visual row out in the
//! widest horizontal gap the exclusions leave at that row's height. A row with
//! no usable gap moves down below the exclusion that blocks it; the space it
//! skipped is the row's [`LayoutLine::top_gap`](crate::LayoutLine::top_gap).

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

/// A rectangle text flows around, in buffer content coordinates: `x` from the
/// buffer's leading edge and `y` from the top of its first line, or of its
/// anchor [`line`](Self::line), before scrolling, in the same units as the
/// buffer's metrics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Exclusion {
    /// Left edge.
    pub x: f32,
    /// Top edge, from the top of [`Self::line`] when it is set.
    pub y: f32,
    /// Width; an exclusion with no width excludes nothing.
    pub width: f32,
    /// Height; an exclusion with no height excludes nothing.
    pub height: f32,
    /// The line whose top [`Self::y`] is measured from, so the exclusion
    /// moves with that line as the text above it changes, like a float in its
    /// paragraph; `None` measures from the top of the buffer. Lines before the
    /// anchor never flow around it, and an anchor past the last line is
    /// ignored.
    pub line: Option<usize>,
}

impl Exclusion {
    /// An exclusion at `x`, `y` of `width` by `height`.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            line: None,
        }
    }

    /// An exclusion anchored to `line`: `y` is measured from that line's top.
    pub const fn anchored(line: usize, x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            line: Some(line),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }

    pub(crate) const fn bottom(&self) -> f32 {
        self.y + self.height
    }
}

/// Where one visual row sits: its leading inset, the width it wraps at, and
/// the space it skipped above itself to get past a blocking exclusion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RowBand {
    pub left: f32,
    pub width: f32,
    pub gap: f32,
}

/// The exclusions a paragraph lays out against, and the paragraph's top in the
/// same coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Flow<'a> {
    pub exclusions: &'a [Exclusion],
    pub top: f32,
}

/// The bands of the rows of a paragraph whose rows are `line_height` tall,
/// until the rows are below every exclusion; rows past the end of `out` take
/// the full `width` and skip nothing.
///
/// A gap narrower than `min_width` is not used: the row moves below the
/// exclusion instead of wrapping a few glyphs into a sliver.
pub(crate) fn row_bands(
    flow: Flow<'_>,
    line_height: f32,
    width: f32,
    min_width: f32,
    out: &mut Vec<RowBand>,
) {
    out.clear();
    let bottom = flow
        .exclusions
        .iter()
        .filter(|e| !e.is_empty())
        .map(Exclusion::bottom)
        .fold(f32::NEG_INFINITY, f32::max);
    if !(line_height > 0.0) || !bottom.is_finite() {
        return;
    }
    let min_width = min_width.min(width);
    let mut y = flow.top;
    while y < bottom {
        let mut gap = 0.0;
        loop {
            let (left, free, blocked_until) = widest_gap(flow.exclusions, y, line_height, width);
            if free >= min_width || blocked_until <= y {
                out.push(RowBand {
                    left,
                    width: free,
                    gap,
                });
                break;
            }
            gap += blocked_until - y;
            y = blocked_until;
        }
        y += line_height;
    }
    // Rows that need nothing are left to the full-width default.
    while out
        .last()
        .is_some_and(|band| band.left == 0.0 && band.width == width && band.gap == 0.0)
    {
        out.pop();
    }
}

/// The widest gap in `0..width` the exclusions overlapping the row at `y`
/// leave, as `(left, width)`, the leftmost of equally wide ones; and the
/// highest bottom among those exclusions (or `y` when none overlaps).
fn widest_gap(exclusions: &[Exclusion], y: f32, height: f32, width: f32) -> (f32, f32, f32) {
    let mut spans: Vec<(f32, f32)> = Vec::new();
    let mut blocked_until = f32::INFINITY;
    for e in exclusions {
        if e.is_empty() || e.y >= y + height || e.bottom() <= y {
            continue;
        }
        let start = e.x.max(0.0);
        let end = (e.x + e.width).min(width);
        if end <= start {
            continue;
        }
        spans.push((start, end));
        blocked_until = blocked_until.min(e.bottom());
    }
    if spans.is_empty() {
        return (0.0, width, y);
    }
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut best = (0.0, 0.0);
    let mut cursor = 0.0;
    for (start, end) in spans {
        if start - cursor > best.1 {
            best = (cursor, start - cursor);
        }
        cursor = f32::max(cursor, end);
    }
    if width - cursor > best.1 {
        best = (cursor, width - cursor);
    }
    (best.0, best.1, blocked_until)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bands(exclusions: &[Exclusion], top: f32) -> Vec<RowBand> {
        let mut out = Vec::new();
        row_bands(Flow { exclusions, top }, 10.0, 100.0, 20.0, &mut out);
        out
    }

    #[test]
    fn rows_beside_an_exclusion_take_the_other_side_and_full_width_below() {
        let out = bands(&[Exclusion::new(0.0, 0.0, 40.0, 25.0)], 0.0);
        assert_eq!(
            out,
            vec![
                RowBand {
                    left: 40.0,
                    width: 60.0,
                    gap: 0.0
                };
                3
            ],
            "rows 0..30 overlap 0..25, the fourth row is past it"
        );
        let right = bands(&[Exclusion::new(70.0, 0.0, 30.0, 10.0)], 0.0);
        assert_eq!(
            right,
            vec![RowBand {
                left: 0.0,
                width: 70.0,
                gap: 0.0
            }]
        );
    }

    #[test]
    fn a_paragraph_lower_down_starts_counting_at_its_top() {
        assert!(bands(&[Exclusion::new(0.0, 0.0, 40.0, 25.0)], 30.0).is_empty());
        assert_eq!(
            bands(&[Exclusion::new(0.0, 0.0, 40.0, 25.0)], 20.0).len(),
            1
        );
    }

    #[test]
    fn a_row_with_no_usable_gap_moves_below_the_exclusion() {
        let out = bands(&[Exclusion::new(5.0, 0.0, 90.0, 25.0)], 0.0);
        assert_eq!(
            out,
            vec![RowBand {
                left: 0.0,
                width: 100.0,
                gap: 25.0
            }]
        );
    }
}
