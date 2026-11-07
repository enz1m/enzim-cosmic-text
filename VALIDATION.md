# Validation

Linux x86_64, Rust 1.95.0. The cosmic-text base remains enzUI's pinned revision.

- 20 tests pass across exclusion flow, wrap clamping, mixed-direction widths,
  emoji presentation, fontconfig fallback, raster preferences and padded hit testing.
- Runtime Rust source matches the installed fork after modification notices.
- The raster test uses the existing Inter fixture in this repository instead of
  an iced checkout-relative Fira Sans path. That change is in the font-rendering
  feature commit, and makes the standalone test build self-contained.
- Strict Clippy reports the existing negated floating-point comparison in
  `src/flow.rs:105`; runtime code is retained unchanged.
- All 34 upstream LFS font/image objects are SHA-256 checked, validated against their upstream pointer hashes.
  Their original pointer files and license texts are retained.

```sh
cargo +1.95.0 test --locked --test flow_exclusions --test wrapped_clamp_space --test mixed_direction_width --test emoji_presentation --test fontconfig_script_fallback --test raster_preferences --test hit_padding
```

Source provenance, upstream ancestry and all fork signatures are checked for
publication. Existing applications are not repinned by this source preparation.
