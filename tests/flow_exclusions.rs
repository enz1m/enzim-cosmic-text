// Modified by enzUI contributors. See FORK.md.
//! Text flows around exclusion rectangles: rows beside one wrap in the gap it
//! leaves, rows below it return to the full width, a row with no usable gap
//! moves below it, and hit testing, edits and moving the exclusion follow.

use cosmic_text::{
    Attrs, AttrsList, Buffer, BufferLine, Exclusion, Family, FontSystem, LineEnding, Metrics,
    Shaping, Wrap,
};

const WIDTH: f32 = 300.0;
const LINE: f32 = 20.0;
const TEXT: &str = "The quick brown fox jumps over the lazy dog while the editor keeps one \
paragraph flowing beside the picture and then back to the full width of the column below it, \
which is what a note with an image dropped against its text should look like.";

fn attrs() -> Attrs<'static> {
    Attrs::new().family(Family::Name("Inter"))
}

fn fonts() -> FontSystem {
    let mut fonts = FontSystem::new();
    fonts
        .db_mut()
        .load_font_file(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fonts/Inter-Regular.ttf"
        ))
        .expect("test font");
    fonts
}

fn buffer(fonts: &mut FontSystem, text: &str, exclusions: &[Exclusion]) -> Buffer {
    let mut buffer = Buffer::new(fonts, Metrics::new(14.0, LINE));
    buffer.set_size(Some(WIDTH), Some(1000.0));
    buffer.set_wrap(Wrap::Word);
    buffer.set_text(text, &attrs(), Shaping::Advanced, None);
    buffer.set_exclusions(exclusions);
    buffer.shape_until_scroll(fonts, false);
    buffer
}

/// Each row's line index, top and horizontal extent of its glyphs.
fn rows(buffer: &Buffer) -> Vec<(usize, f32, f32, f32)> {
    buffer
        .layout_runs()
        .filter(|run| !run.glyphs.is_empty())
        .map(|run| {
            let left = run.glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
            let right = run
                .glyphs
                .iter()
                .map(|g| g.x + g.w)
                .fold(f32::NEG_INFINITY, f32::max);
            (run.line_i, run.line_top, left, right)
        })
        .collect()
}

#[test]
fn rows_beside_an_exclusion_wrap_in_its_gap_and_return_to_full_width_below() {
    let mut fonts = fonts();
    let left = Exclusion::new(0.0, 0.0, 120.0, 50.0);
    let rows = rows(&buffer(&mut fonts, TEXT, &[left]));
    assert!(rows.len() > 4, "{rows:?}");
    for &(_, top, start, end) in &rows {
        if top < 50.0 {
            assert!(
                start >= 120.0,
                "a row at {top} starts beside the exclusion: {rows:?}"
            );
            assert!(end <= WIDTH + 0.5, "{rows:?}");
        } else {
            assert!(
                start < 1.0,
                "a row at {top} starts at the leading edge: {rows:?}"
            );
        }
    }
    assert!(
        rows.iter()
            .any(|&(_, top, _, end)| top >= 50.0 && end > WIDTH - 120.0),
        "rows below use the width the exclusion took: {rows:?}"
    );

    let right = Exclusion::new(200.0, 0.0, 100.0, 30.0);
    let flowed = self::rows(&buffer(&mut fonts, TEXT, &[right]));
    for &(_, top, _, end) in &flowed {
        if top < 30.0 {
            assert!(
                end <= 200.5,
                "a row at {top} ends before the exclusion: {flowed:?}"
            );
        }
    }
}

#[test]
fn a_row_left_no_usable_gap_moves_below_and_a_click_there_lands_on_it() {
    let mut fonts = fonts();
    let wide = Exclusion::new(10.0, 0.0, 280.0, 30.0);
    let buffer = buffer(&mut fonts, TEXT, &[wide]);
    let rows = rows(&buffer);
    assert!(
        rows[0].1 >= 30.0,
        "the first row starts below the exclusion: {rows:?}"
    );
    assert!(rows[0].2 < 1.0, "{rows:?}");
    let hit = buffer.hit(0.0, 10.0).expect("a hit");
    assert_eq!(
        (hit.line, hit.index),
        (0, 0),
        "a click in the skipped space lands on the row below it"
    );
}

#[test]
fn a_click_beside_the_exclusion_lands_in_the_flowed_row() {
    let mut fonts = fonts();
    let buffer = buffer(&mut fonts, TEXT, &[Exclusion::new(0.0, 0.0, 120.0, 50.0)]);
    let first_row_end = buffer
        .layout_runs()
        .next()
        .and_then(|run| run.glyphs.last())
        .map(|glyph| glyph.end)
        .expect("a first row");
    let inside = buffer.hit(121.0, 10.0).expect("a hit");
    assert_eq!(
        inside.index, 0,
        "the row's first glyph sits at the exclusion's edge"
    );
    let middle = buffer.hit(210.0, 10.0).expect("a hit");
    assert!(
        middle.index > 0 && middle.index <= first_row_end,
        "{middle:?} is inside the first row (..{first_row_end})"
    );
}

#[test]
fn edits_above_and_moving_the_exclusion_lay_the_rows_out_again() {
    let mut fonts = fonts();
    let text = format!("Short line.\n{TEXT}");
    let exclusion = Exclusion::new(0.0, 0.0, 120.0, 70.0);
    let mut buffer = buffer(&mut fonts, &text, &[exclusion]);
    let second: Vec<_> = rows(&buffer).into_iter().filter(|r| r.0 == 1).collect();
    assert!(second[0].2 >= 120.0 && second[0].1 == LINE, "{second:?}");

    // Two lines inserted above push the paragraph down to 60: its first row
    // is still beside the exclusion, its second is past its bottom.
    for _ in 0..2 {
        buffer.lines.insert(
            0,
            BufferLine::new(
                "Above.",
                LineEnding::Lf,
                AttrsList::new(&attrs()),
                Shaping::Advanced,
            ),
        );
    }
    buffer.note_encapsulated_line_mutation();
    buffer.shape_until_scroll(&mut fonts, false);
    let pushed: Vec<_> = rows(&buffer).into_iter().filter(|r| r.0 == 3).collect();
    assert_eq!(pushed[0].1, 3.0 * LINE, "{pushed:?}");
    assert!(
        pushed[0].2 >= 120.0,
        "the row at {} is still beside it",
        pushed[0].1
    );
    assert!(
        pushed[1].2 < 1.0,
        "the row at {} is below it: {pushed:?}",
        pushed[1].1
    );

    buffer.set_exclusions(&[]);
    buffer.shape_until_scroll(&mut fonts, false);
    assert!(
        rows(&buffer).iter().all(|r| r.2 < 1.0),
        "without exclusions every row starts at the edge"
    );
}

#[test]
fn an_anchored_exclusion_moves_with_its_line_and_leaves_the_lines_before_it_alone() {
    let mut fonts = fonts();
    let text = format!("{TEXT}\n{TEXT}");
    let float = Exclusion::anchored(1, 0.0, 0.0, 120.0, 30.0);
    let mut buffer = buffer(&mut fonts, &text, &[float]);
    let before = rows(&buffer);
    assert!(
        before.iter().filter(|r| r.0 == 0).all(|r| r.2 < 1.0),
        "the line before the anchor keeps the full width: {before:?}"
    );
    let first = before
        .iter()
        .find(|r| r.0 == 1)
        .expect("line 1 is laid out");
    assert!(
        first.2 >= 120.0,
        "the anchor line flows beside it: {before:?}"
    );
    let resolved = buffer.resolved_exclusions(&mut fonts);
    assert_eq!(resolved.len(), 1);
    assert_eq!(
        resolved[0].map(|e| e.y),
        Some(first.1),
        "it sits at its line's top"
    );

    buffer.lines.insert(
        0,
        BufferLine::new(
            "Above.",
            LineEnding::Lf,
            AttrsList::new(&attrs()),
            Shaping::Advanced,
        ),
    );
    buffer.note_encapsulated_line_mutation();
    buffer.shape_until_scroll(&mut fonts, false);
    // The anchor index is the caller's: it now names the inserted line's
    // successor, so the caller moves it with the edit.
    buffer.set_exclusions(&[Exclusion::anchored(2, 0.0, 0.0, 120.0, 30.0)]);
    buffer.shape_until_scroll(&mut fonts, false);
    let after = rows(&buffer);
    let moved = after.iter().find(|r| r.0 == 2).expect("line 2 is laid out");
    assert_eq!(moved.1, first.1 + LINE, "{after:?}");
    assert!(moved.2 >= 120.0, "{after:?}");
    assert_eq!(
        buffer.resolved_exclusions(&mut fonts)[0].map(|e| e.y),
        Some(moved.1)
    );
}
