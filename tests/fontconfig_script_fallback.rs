// Modified by enzUI contributors. See FORK.md.
//! A script the primary family lacks falls back to the face fontconfig
//! picks for its language (enzICED), as GTK and Qt applications do.
#![cfg(all(feature = "fontconfig", target_os = "linux"))]

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

fn fc_match(pattern: &str) -> Option<String> {
    let output = std::process::Command::new("fc-match")
        .args(["-f", "%{family[0]}", pattern])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The families that draw the non-space characters of `text` set in `family`.
fn faces(fonts: &mut FontSystem, family: &str, text: &str) -> Vec<String> {
    let mut buffer = Buffer::new(fonts, Metrics::new(16.0, 20.0));
    buffer.set_size(Some(800.0), Some(100.0));
    buffer.set_text(
        text,
        &Attrs::new().family(Family::Name(family)),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(fonts, false);
    buffer
        .layout_runs()
        .flat_map(|run| run.glyphs.to_vec())
        .filter(|glyph| !text[glyph.start..glyph.end].trim().is_empty())
        .filter_map(|glyph| {
            fonts
                .db()
                .face(glyph.font_id)
                .map(|face| face.families[0].0.clone())
        })
        .collect()
}

#[test]
fn scripts_a_latin_family_lacks_use_the_fontconfig_face() {
    if fc_match("sans-serif").is_none() {
        return;
    }
    let mut fonts = FontSystem::new();
    let covers = |fonts: &mut FontSystem, family: &str, c: char| {
        let id = fonts.db().query(&cosmic_text::fontdb::Query {
            families: &[cosmic_text::fontdb::Family::Name(family)],
            ..Default::default()
        });
        id.and_then(|id| fonts.get_font(id, cosmic_text::Weight::NORMAL))
            .map(|font| font.as_swash().charmap().map(c) != 0)
    };
    let samples = [
        ("ar", "اليوم هو"),
        ("ja", "日本語のテキスト"),
        ("hi", "हिन्दी पाठ"),
        ("th", "ภาษาไทย"),
    ];
    let mut checked = 0;
    for (language, text) in samples {
        let first = text.chars().next().unwrap();
        let Some(expected) = fc_match(&format!("sans-serif:lang={language}")) else {
            continue;
        };
        if covers(&mut fonts, &expected, first) != Some(true) {
            // fontconfig itself has nothing for this language here.
            continue;
        }
        let Some(latin) = ["Inter", "Cantarell", "Adwaita Sans", "Liberation Sans"]
            .into_iter()
            .find(|family| covers(&mut fonts, family, first) == Some(false))
        else {
            continue;
        };
        let drawn = faces(&mut fonts, latin, text);
        assert!(
            !drawn.is_empty() && drawn.iter().all(|face| *face == expected),
            "{latin} + {language} drew with {drawn:?}, fontconfig picks {expected}"
        );
        checked += 1;
    }
    assert!(checked > 0 || fc_match("sans-serif:lang=ar").is_none());
}
