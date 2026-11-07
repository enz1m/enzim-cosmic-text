// Modified by enzUI contributors. See FORK.md.
use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent};

fn fonts(with_emoji: bool) -> FontSystem {
    let installed = FontSystem::new();
    let mut db = cosmic_text::fontdb::Database::new();
    let families: &[&str] = if with_emoji {
        &["DejaVu Sans", "Noto Color Emoji"]
    } else {
        &["DejaVu Sans"]
    };
    for family in families {
        let id = installed
            .db()
            .query(&cosmic_text::fontdb::Query {
                families: &[cosmic_text::fontdb::Family::Name(family)],
                ..Default::default()
            })
            .unwrap_or_else(|| panic!("required native font missing: {family}"));
        let data = installed
            .db()
            .with_face_data(id, |bytes, _| bytes.to_vec())
            .unwrap();
        db.load_font_data(data);
    }
    FontSystem::new_with_locale_and_db("en-US".into(), db)
}

fn glyphs(fonts: &mut FontSystem, text: &str) -> Vec<cosmic_text::LayoutGlyph> {
    let mut buffer = Buffer::new(fonts, Metrics::new(26.0, 35.0));
    buffer.set_size(Some(1200.0), Some(300.0));
    buffer.set_text(
        text,
        &Attrs::new().family(Family::Name("DejaVu Sans")),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fonts, false);
    buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.to_vec())
        .collect()
}

#[test]
fn emoji_presentation_uses_color_without_changing_text_or_breaking_clusters() {
    let mut fonts = fonts(true);
    let text = "A😀🙂☺️🤣☕👩🏽‍💻🇮🇪1️⃣Z ☺︎ ☕︎";
    let shaped = glyphs(&mut fonts, text);
    let mut cache = SwashCache::new();
    for cluster in ["😀", "🙂", "☺️", "🤣", "☕", "👩🏽‍💻", "🇮🇪", "1️⃣"]
    {
        let start = text.find(cluster).unwrap();
        let end = start + cluster.len();
        let selected: Vec<_> = shaped
            .iter()
            .filter(|glyph| glyph.start >= start && glyph.start < end)
            .collect();
        assert_eq!(
            selected.len(),
            1,
            "one shaped glyph for the joined cluster {cluster}"
        );
        let glyph = selected[0];
        assert_eq!((glyph.start, glyph.end), (start, end));
        assert_ne!(glyph.glyph_id, 0, "missing {cluster}");
        let face = fonts.db().face(glyph.font_id).unwrap();
        assert!(
            face.families
                .iter()
                .any(|(family, _)| family == "Noto Color Emoji"),
            "wrong face for {cluster}: {:?}",
            face.families
        );
        let image = cache
            .get_image(&mut fonts, glyph.physical((0.0, 0.0), 1.0).cache_key)
            .as_ref()
            .expect("painted emoji");
        assert_eq!(image.content, SwashContent::Color);
        assert!(image.placement.width > 10 && image.placement.height > 10);
        let colored = image
            .data
            .chunks_exact(4)
            .filter(|pixel| {
                pixel[3] > 128
                    && pixel[..3].iter().max().unwrap() - pixel[..3].iter().min().unwrap() > 40
            })
            .count();
        assert!(colored > 30, "no useful color ink for {cluster}: {colored}");
    }
    for cluster in ["A", "Z", "☺︎", "☕︎"] {
        let start = text.find(cluster).unwrap();
        let glyph = shaped
            .iter()
            .find(|glyph| glyph.start == start)
            .expect("text control");
        assert_ne!(glyph.glyph_id, 0);
        assert!(
            fonts
                .db()
                .face(glyph.font_id)
                .unwrap()
                .families
                .iter()
                .any(|(family, _)| family == "DejaVu Sans"),
            "text presentation left its chosen family: {cluster}"
        );
        assert_eq!(
            cache
                .get_image(&mut fonts, glyph.physical((0.0, 0.0), 1.0).cache_key)
                .as_ref()
                .expect("painted text")
                .content,
            SwashContent::Mask
        );
    }
}

#[test]
fn emoji_presentation_retains_normal_fallback_without_a_color_provider() {
    let mut fonts = fonts(false);
    let shaped = glyphs(&mut fonts, "A☕Z");
    assert_eq!(shaped.len(), 3);
    let mut cache = SwashCache::new();
    for glyph in shaped {
        assert_ne!(glyph.glyph_id, 0);
        assert!(
            fonts
                .db()
                .face(glyph.font_id)
                .unwrap()
                .families
                .iter()
                .any(|(family, _)| family == "DejaVu Sans")
        );
        assert_eq!(
            cache
                .get_image(&mut fonts, glyph.physical((0.0, 0.0), 1.0).cache_key)
                .as_ref()
                .unwrap()
                .content,
            SwashContent::Mask
        );
    }
}

#[test]
fn emoji_presentation_splits_an_unbroken_text_word_at_the_grapheme() {
    let mut fonts = fonts(true);
    let text = "a©️b";
    let shaped = glyphs(&mut fonts, text);
    let copyright = shaped
        .iter()
        .filter(|glyph| glyph.start == 1)
        .collect::<Vec<_>>();
    assert_eq!(copyright.len(), 1);
    assert_eq!(copyright[0].end, 1 + "©️".len());
    assert!(
        fonts
            .db()
            .face(copyright[0].font_id)
            .unwrap()
            .families
            .iter()
            .any(|(family, _)| family == "Noto Color Emoji")
    );
    let mut cache = SwashCache::new();
    let image = cache
        .get_image(&mut fonts, copyright[0].physical((0.0, 0.0), 1.0).cache_key)
        .as_ref()
        .unwrap();
    assert_eq!(image.content, SwashContent::Color);
    assert!(
        image
            .data
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 128)
            .count()
            > 30
    );
    for position in [0, 1 + "©️".len()] {
        let glyph = shaped.iter().find(|glyph| glyph.start == position).unwrap();
        assert!(
            fonts
                .db()
                .face(glyph.font_id)
                .unwrap()
                .families
                .iter()
                .any(|(family, _)| family == "DejaVu Sans")
        );
    }
}
