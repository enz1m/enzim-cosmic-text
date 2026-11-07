# Fork provenance

- Upstream: https://github.com/hecrj/cosmic-text
- Base: `6adc2364289fd241daed751948a03d0aea5a31ca`
- Branch: `enzim`; crate names and upstream ancestry are unchanged.
- License: MIT OR Apache-2.0. Upstream license files and notices are retained.
- Fork author and signer: Bernard, [enz1m](https://github.com/enz1m).

## Changes

- text: Reuse prepared layout for pointer actions
- text: Bound editor frame work
- text: Bound drag and highlight work
- text: Render editable text decorations
- text: Align editable line geometry
- text: Snap glyphs to the nearest pixel
- text: Skip consumed wrap separators
- text: Prefer installed color emoji
- text: Honor line height when clamping ellipsis
- text: Remove the unused truncation helper
- text: Bound editor caches and undo history
- font: Cache weight axes per face
- text: Measure initial bidirectional words
- text: Satisfy standalone lint checks
- font: Follow desktop font rendering preferences
- text: Expose a line shaping strategy
- text: Flow lines around exclusion rectangles
- font: Preserve the requested family during weight fallback

The base is the cosmic-text revision already used by enzUI, including its
existing hecrj changes. Test fonts and images keep upstream Git LFS storage
and their original licenses. The upstream Git LFS objects are retained.

Unicode property data retains `LICENSE-Unicode`; font fixtures retain their
upstream OFL notices.
