#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};
use core::hash::{Hash, Hasher};
use core::ops::Range;

use crate::{AttrsOwned, BuildHasher, ShapeGlyph};
use hashbrown::{Equivalent, HashMap};

/// Key for caching shape runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeRunKey {
    pub text: String,
    pub default_attrs: AttrsOwned,
    pub attrs_spans: Vec<(Range<usize>, AttrsOwned)>,
    /// Whether the run is shaped right to left, which mirrors and orders its glyphs.
    pub rtl: bool,
}

#[derive(Hash)]
struct RunKeyRef<'a> {
    text: &'a str,
    default_attrs: &'a AttrsOwned,
    attrs_spans: &'a [(Range<usize>, AttrsOwned)],
    rtl: bool,
}

impl Equivalent<ShapeRunKey> for RunKeyRef<'_> {
    fn equivalent(&self, key: &ShapeRunKey) -> bool {
        self.text == key.text
            && self.default_attrs == &key.default_attrs
            && self.attrs_spans == key.attrs_spans
            && self.rtl == key.rtl
    }
}

impl Hash for ShapeRunKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        RunKeyRef {
            text: &self.text,
            default_attrs: &self.default_attrs,
            attrs_spans: &self.attrs_spans,
            rtl: self.rtl,
        }
        .hash(state);
    }
}

/// A helper structure for caching shape runs.
#[derive(Clone, Default)]
pub struct ShapeRunCache {
    age: u64,
    cache: HashMap<ShapeRunKey, (u64, Vec<ShapeGlyph>), BuildHasher>,
}

impl ShapeRunCache {
    /// Entries kept before an insertion empties the cache, bounding it between trims.
    pub const CAPACITY: usize = 4096;

    /// Get cache item, updating age if found
    pub fn get(&mut self, key: &ShapeRunKey) -> Option<&Vec<ShapeGlyph>> {
        self.get_key(key)
    }

    pub(crate) fn get_plain(
        &mut self,
        text: &str,
        default_attrs: &AttrsOwned,
        rtl: bool,
    ) -> Option<&[ShapeGlyph]> {
        let key = RunKeyRef {
            text,
            default_attrs,
            attrs_spans: &[],
            rtl,
        };
        self.get_key(&key).map(Vec::as_slice)
    }

    fn get_key(&mut self, key: &(impl Equivalent<ShapeRunKey> + Hash)) -> Option<&Vec<ShapeGlyph>> {
        self.cache.get_mut(key).map(|(age, glyphs)| {
            *age = self.age;
            &*glyphs
        })
    }

    /// Insert cache item with current age
    pub fn insert(&mut self, key: ShapeRunKey, glyphs: Vec<ShapeGlyph>) {
        if self.cache.len() >= Self::CAPACITY {
            self.cache.clear();
        }
        self.cache.insert(key, (self.age, glyphs));
    }

    /// Removes every entry.
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Number of cached runs.
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Whether no run is cached.
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Remove anything in the cache with an age older than `keep_ages`
    pub fn trim(&mut self, keep_ages: u64) {
        self.cache
            .retain(|_key, (age, _glyphs)| *age + keep_ages >= self.age);
        // Increase age
        self.age += 1;
    }
}

impl core::fmt::Debug for ShapeRunCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("ShapeRunCache").finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{ShapeRunCache, ShapeRunKey};
    use crate::{Attrs, AttrsOwned, Color};
    #[cfg(not(feature = "std"))]
    use alloc::{vec, vec::Vec};

    #[test]
    fn borrowed_lookup_respects_spans_direction_and_age() {
        let defaults = AttrsOwned::new(&Attrs::new());
        let styled = AttrsOwned::new(&Attrs::new().color(Color::rgb(0, 1, 2)));
        let key = ShapeRunKey {
            text: "katakana".into(),
            default_attrs: defaults.clone(),
            attrs_spans: vec![(0..1, styled.clone())],
            rtl: false,
        };
        let mut cache = ShapeRunCache::default();
        cache.insert(key.clone(), Vec::new());
        assert!(cache.get_plain(&key.text, &defaults, false).is_none());

        let plain_rtl = ShapeRunKey {
            attrs_spans: Vec::new(),
            rtl: true,
            ..key
        };
        cache.insert(plain_rtl.clone(), Vec::new());
        assert!(cache.get_plain(&plain_rtl.text, &defaults, false).is_none());
        assert!(cache.get_plain(&plain_rtl.text, &styled, true).is_none());
        cache.trim(0);
        assert!(cache.get_plain(&plain_rtl.text, &defaults, true).is_some());
        cache.trim(0);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&plain_rtl).is_some());
    }
}
