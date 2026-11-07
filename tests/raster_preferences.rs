// Modified by enzUI contributors. See FORK.md.
//! Glyphs follow the desktop's hinting and antialiasing choice (enzICED).
//!
//! One test in its own target: the choice is process-wide.

use cosmic_text::{
    Attrs, Buffer, CacheKeyFlags, Family, FontSystem, Metrics, Shaping, SwashCache, SwashContent,
};

fn fonts() -> FontSystem {
    let mut db = cosmic_text::fontdb::Database::new();
    db.load_font_data(include_bytes!("../fonts/Inter-Regular.ttf").to_vec());
    FontSystem::new_with_locale_and_db("en-US".into(), db)
}

/// The coverage masks and cache flags of `text`'s glyphs.
fn masks(fonts: &mut FontSystem, cache: &mut SwashCache) -> Vec<(CacheKeyFlags, Vec<u8>)> {
    let mut buffer = Buffer::new(fonts, Metrics::new(15.0, 20.0));
    buffer.set_size(Some(400.0), Some(40.0));
    buffer.set_text(
        "Rasterized",
        &Attrs::new().family(Family::Name("Inter")),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fonts, false);
    let glyphs: Vec<_> = buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.to_vec())
        .collect();
    glyphs
        .iter()
        .filter_map(|glyph| {
            let key = glyph.physical((0.0, 0.0), 1.0).cache_key;
            let image = cache.get_image_uncached(fonts, key)?;
            (image.content == SwashContent::Mask).then_some((key.flags, image.data))
        })
        .collect()
}

#[test]
fn glyphs_follow_the_raster_preferences() {
    let mut fonts = fonts();
    let mut cache = SwashCache::new();
    let partial = |masks: &[(CacheKeyFlags, Vec<u8>)]| {
        masks
            .iter()
            .flat_map(|(_, data)| data)
            .filter(|c| **c != 0 && **c != 255)
            .count()
    };

    let smooth = masks(&mut fonts, &mut cache);
    assert!(!smooth.is_empty());
    assert!(partial(&smooth) > 0, "antialiased edges are partly covered");
    assert!(smooth.iter().all(|(flags, _)| flags.is_empty()));

    cosmic_text::set_raster_preferences(false, false);
    let crisp = masks(&mut fonts, &mut cache);
    cosmic_text::set_raster_preferences(true, true);
    assert_eq!(partial(&crisp), 0, "no antialiasing: every pixel on or off");
    assert!(crisp.iter().any(|(_, data)| data.contains(&255)));
    assert!(crisp.iter().all(|(flags, _)| {
        flags.contains(CacheKeyFlags::MONOCHROME | CacheKeyFlags::DISABLE_HINTING)
    }));
}
