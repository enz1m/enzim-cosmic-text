// Modified by enzUI contributors. See FORK.md.
use cosmic_text::{
    Attrs, Buffer, Ellipsize, EllipsizeHeightLimit, FontSystem, Metrics, Shaping, Wrap,
};

const PROSE: &str = "An ellipsis bounded by height belongs on the last line that fits. \
Alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike \
november oscar papa quebec romeo sierra tango uniform victor whiskey xray yankee \
zulu, and then the alphabet once more for good measure.";

/// Each laid-out run's top, bottom and whether it carries the ellipsis.
fn runs(font_size: f32, line_height: f32, height: f32) -> Vec<(f32, f32, bool)> {
    let mut fonts = FontSystem::new();
    let mut buffer = Buffer::new(&mut fonts, Metrics::new(font_size, line_height));
    buffer.set_size(Some(220.0), Some(height));
    buffer.set_wrap(Wrap::Word);
    buffer.set_ellipsize(Ellipsize::End(EllipsizeHeightLimit::Height(height)));
    buffer.set_text(PROSE, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut fonts, false);
    buffer
        .layout_runs()
        .map(|run| {
            let ellipsized = run.glyphs.iter().any(|glyph| glyph.start == glyph.end);
            (run.line_top, run.line_top + run.line_height, ellipsized)
        })
        .collect()
}

#[test]
fn the_ellipsis_lands_on_the_last_line_that_fits_the_height() {
    // Line heights unlike the font size, including one that fills the height
    // exactly: the limit counts lines of the buffer's line height.
    for (font_size, line_height) in [(13.0, 16.9), (14.0, 18.2), (14.0, 20.0), (16.0, 20.8)] {
        let height = 60.0;
        let laid = runs(font_size, line_height, height);
        let last = laid.last().copied().expect("the prose lays out");
        assert!(
            last.2,
            "{font_size}/{line_height}: the last laid-out line carries the ellipsis: {laid:?}"
        );
        assert!(
            last.1 <= height + 0.01,
            "{font_size}/{line_height}: the ellipsized line ends inside the height: {laid:?}"
        );
        let fitting = (height / line_height + 0.001).floor() as usize;
        assert_eq!(
            laid.len(),
            fitting,
            "{font_size}/{line_height}: every line that fits is laid out: {laid:?}"
        );
    }
}
