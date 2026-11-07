// Modified by enzUI contributors. See FORK.md.
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Wrap};

#[test]
fn mixed_direction_width_contains_every_emitted_glyph() {
    let mut fonts = FontSystem::new();
    for text in [
        "1:05 م",
        "1:05 PM",
        "١:٠٥ م",
        "12:34 א",
        "م 1:05",
        "موعد 1:05 اليوم",
    ] {
        let mut buffer = Buffer::new(&mut fonts, Metrics::new(13.0, 17.0));
        buffer.set_size(None, None);
        buffer.set_wrap(Wrap::None);
        buffer.set_text(
            text,
            &Attrs::new().family(Family::Name("DejaVu Sans")),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut fonts, false);
        let runs: Vec<_> = buffer.layout_runs().collect();
        assert_eq!(runs.len(), 1, "{text}");
        let run = &runs[0];
        assert!(!run.glyphs.is_empty(), "{text}");
        for glyph in run.glyphs {
            assert!(glyph.x >= -0.01, "{text}: {glyph:?}");
            assert!(
                glyph.x + glyph.w <= run.line_w + 0.01,
                "{text}: {glyph:?}, width {}",
                run.line_w
            );
        }
    }
}
