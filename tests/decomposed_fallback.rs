// Modified by enzUI contributors. See FORK.md.
use cosmic_text::{fontdb, Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

/// A TrueType font named `family` with one empty glyph for each of `chars`.
fn font(family: &str, chars: &[char]) -> Vec<u8> {
    fn be16(out: &mut Vec<u8>, values: &[u16]) {
        for value in values {
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
    let glyphs = u16::try_from(chars.len() + 1).unwrap();

    let mut head = Vec::new();
    be16(&mut head, &[1, 0, 1, 0, 0, 0, 0x5F0F, 0x3CF5, 0x000B, 1000]);
    head.extend_from_slice(&[0; 16]);
    be16(&mut head, &[0, 0, 500, 700, 0, 8, 2, 0, 0]);

    let mut hhea = Vec::new();
    be16(
        &mut hhea,
        &[1, 0, 800, (-200i16) as u16, 0, 500, 0, 0, 500, 1, 0, 0],
    );
    be16(&mut hhea, &[0, 0, 0, 0, 0, glyphs]);

    let mut maxp = Vec::new();
    be16(&mut maxp, &[1, 0, glyphs]);
    maxp.extend_from_slice(&[0; 26]);

    let mut hmtx = Vec::new();
    for _ in 0..glyphs {
        be16(&mut hmtx, &[500, 0]);
    }
    let loca = vec![0; 2 * (usize::from(glyphs) + 1)];

    let mut mapped: Vec<(u16, u16)> = chars
        .iter()
        .zip(1..)
        .map(|(&c, glyph)| (u16::try_from(u32::from(c)).unwrap(), glyph))
        .collect();
    mapped.sort_unstable();
    mapped.push((0xFFFF, 0));
    let segments = u16::try_from(mapped.len()).unwrap();
    let mut cmap = Vec::new();
    be16(&mut cmap, &[0, 1, 3, 1, 0, 12, 4, 16 + 8 * segments, 0]);
    let search = 2 * (1 << (15 - segments.leading_zeros()));
    be16(
        &mut cmap,
        &[
            2 * segments,
            search,
            search.trailing_zeros() as u16 - 1,
            2 * segments - search,
        ],
    );
    for &(code, _) in &mapped {
        be16(&mut cmap, &[code]);
    }
    be16(&mut cmap, &[0]);
    for &(code, _) in &mapped {
        be16(&mut cmap, &[code]);
    }
    for &(code, glyph) in &mapped {
        be16(&mut cmap, &[glyph.wrapping_sub(code)]);
    }
    for _ in &mapped {
        be16(&mut cmap, &[0]);
    }

    let names = [(1, family), (2, "Regular"), (4, family), (6, family)];
    let mut name = Vec::new();
    let mut strings = Vec::new();
    be16(
        &mut name,
        &[0, names.len() as u16, 6 + 12 * names.len() as u16],
    );
    for (id, text) in names {
        let encoded: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        be16(
            &mut name,
            &[3, 1, 0x409, id, encoded.len() as u16, strings.len() as u16],
        );
        strings.extend(encoded);
    }
    name.extend(strings);

    let mut os2 = Vec::new();
    be16(&mut os2, &[4, 500, 400, 5, 0]);
    os2.extend_from_slice(&[0; 52]);
    be16(
        &mut os2,
        &[0x40, 0, 0xFFFF, 800, (-200i16) as u16, 0, 800, 200],
    );
    os2.extend_from_slice(&[0; 18]);

    let mut post = Vec::new();
    be16(&mut post, &[3, 0, 0, 0, (-100i16) as u16, 50]);
    post.extend_from_slice(&[0; 20]);

    let tables: [(&[u8; 4], Vec<u8>); 10] = [
        (b"OS/2", os2),
        (b"cmap", cmap),
        (b"glyf", Vec::new()),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
        (b"name", name),
        (b"post", post),
    ];
    let mut out = Vec::new();
    be16(&mut out, &[1, 0, tables.len() as u16, 128, 3, 32]);
    let mut offset = 12 + 16 * tables.len();
    let mut data = Vec::new();
    for (tag, table) in &tables {
        out.extend_from_slice(*tag);
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&u32::try_from(offset).unwrap().to_be_bytes());
        out.extend_from_slice(&u32::try_from(table.len()).unwrap().to_be_bytes());
        let padded = table.len().next_multiple_of(4);
        data.extend_from_slice(table);
        data.resize(data.len() + padded - table.len(), 0);
        offset += padded;
    }
    out.extend(data);
    out
}

fn assert_drawn_by_fallback(fallback: &[char], expected_glyphs: usize) {
    let mut db = fontdb::Database::new();
    db.load_font_data(font("Primary", &['a']));
    db.load_font_data(font("Decomposing", fallback));
    let mut fonts = FontSystem::new_with_locale_and_db("en-US".into(), db);

    for _ in 0..2 {
        let mut buffer = Buffer::new(&mut fonts, Metrics::new(16.0, 20.0));
        buffer.set_text(
            "a\u{1EC7}",
            &Attrs::new().family(Family::Name("Primary")),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut fonts, false);
        let glyphs: Vec<_> = buffer
            .layout_runs()
            .flat_map(|run| run.glyphs.to_vec())
            .filter(|glyph| glyph.start == 1)
            .collect();
        assert_eq!(glyphs.len(), expected_glyphs, "{fallback:?}: {glyphs:?}");
        for glyph in glyphs {
            assert_ne!(glyph.glyph_id, 0);
            let face = fonts.db().face(glyph.font_id).unwrap();
            assert_eq!(face.families[0].0, "Decomposing");
        }
    }
}

#[test]
fn precomposed_text_falls_back_to_a_face_drawing_its_decomposition() {
    assert_drawn_by_fallback(&['e', '\u{0302}', '\u{0323}'], 3);
}

#[test]
fn precomposed_text_falls_back_to_a_face_drawing_a_composed_prefix() {
    assert_drawn_by_fallback(&['\u{1EB9}', '\u{0302}'], 2);
}
