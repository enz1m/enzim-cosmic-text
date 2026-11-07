// Modified by enzUI contributors. See FORK.md.
use cosmic_text::{
    Attrs, Buffer, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, Metrics, Shaping, Wrap,
};

fn origins(text: &str, width: f32, ellipsize: Ellipsize) -> Vec<Vec<(usize, f32)>> {
    let mut fonts = FontSystem::new();
    let mut buffer = Buffer::new(&mut fonts, Metrics::new(13.0, 17.55));
    buffer.set_size(Some(width), Some(200.0));
    buffer.set_wrap(Wrap::Word);
    buffer.set_ellipsize(ellipsize);
    buffer.set_text(
        text,
        &Attrs::new().family(Family::Name("Liberation Sans")),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(&mut fonts, false);
    buffer
        .layout_runs()
        .map(|run| {
            run.glyphs
                .iter()
                .filter(|glyph| {
                    text.get(glyph.start..glyph.end)
                        .is_some_and(|s| !s.is_empty())
                })
                .map(|glyph| (glyph.start, glyph.x))
                .collect()
        })
        .collect()
}

#[test]
fn wrapped_clamp_space_preserves_the_continuation_origin() {
    for text in [
        "WWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWWW notes are in the shared document for everyone.",
        "Room 4 with Maya, Jon and three others. Notes are in the shared doc.",
        "Prefix שלום חברים זה טקסט ארוך עם מילים רבות כדי לבדוק את השורה השנייה והמשך הטקסט.",
        "שלום חברים זה טקסט ארוך עם מילים רבות כדי לבדוק את השורה השנייה והמשך הטקסט.",
    ] {
        for width in [130.0, 190.0, 240.0, 336.0] {
            let unlimited = origins(text, width, Ellipsize::None);
            assert!(unlimited.len() >= 2);
            assert!(unlimited[1].len() > 2);
            for limit in [
                EllipsizeHeightLimit::Lines(2),
                EllipsizeHeightLimit::Height(35.1),
            ] {
                let clamped = origins(text, width, Ellipsize::End(limit));
                assert_eq!(clamped.len(), 2);
                assert!(clamped[1].len() > 2);
                let source_start = unlimited[1]
                    .iter()
                    .filter(|(start, _)| !text[*start..].chars().next().unwrap().is_whitespace())
                    .map(|(start, _)| *start)
                    .min()
                    .unwrap();
                let expected = unlimited[1]
                    .iter()
                    .find(|(start, _)| *start == source_start)
                    .unwrap()
                    .1;
                let actual = clamped[1]
                    .iter()
                    .find(|(start, _)| *start == source_start)
                    .expect("continuation glyph")
                    .1;
                let first = clamped[1].iter().map(|(start, _)| *start).min().unwrap();
                assert!(
                    !text[first..].chars().next().unwrap().is_whitespace(),
                    "soft-wrap separator starts the clamped continuation: {text}"
                );
                if text.starts_with("Prefix") {
                    continue;
                }
                assert!(
                    (actual - expected).abs() < 0.01,
                    "width {width}, {limit:?}: source {source_start} moved from {expected} to {actual}; {text}"
                );
            }
        }
    }
}

#[test]
fn wrapped_clamp_space_preserves_explicit_initial_indentation() {
    let plain = origins(
        "Room 4 with Maya",
        336.0,
        Ellipsize::End(EllipsizeHeightLimit::Lines(2)),
    );
    let indented = origins(
        "  Room 4 with Maya",
        336.0,
        Ellipsize::End(EllipsizeHeightLimit::Lines(2)),
    );
    assert!(indented[0].iter().find(|(start, _)| *start == 2).unwrap().1 > plain[0][0].1 + 5.0);
}
