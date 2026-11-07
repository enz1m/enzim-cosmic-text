// Modified by enzUI contributors. See FORK.md.
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Font matching through the system's fontconfig (enzICED).
//!
//! GTK and Qt ask fontconfig which family draws a generic name
//! (`sans-serif`) or a language (`sans-serif:lang=ar`), so the user's
//! substitution rules and installed fonts decide. This module asks the same
//! questions. `libfontconfig.so.1` is loaded at run time; without it every
//! query answers `None` and the built-in lists apply.

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void, CStr};
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Mutex, OnceLock, PoisonError};
use unicode_script::Script;

/// What fontconfig answered for a pattern.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontMatch {
    /// The matched family.
    pub family: String,
    /// Whether glyphs are antialiased, when the configuration says.
    pub antialias: Option<bool>,
    /// Whether glyphs are hinted, when the configuration says.
    pub hinting: Option<bool>,
    /// The hint style: 0 none, 1 slight, 2 medium, 3 full.
    pub hint_style: Option<i32>,
}

type Pattern = *mut c_void;

struct Library {
    // Kept loaded for the function pointers below.
    _library: libloading::Library,
    name_parse: unsafe extern "C" fn(*const u8) -> Pattern,
    config_substitute: unsafe extern "C" fn(*mut c_void, Pattern, c_int) -> c_int,
    default_substitute: unsafe extern "C" fn(Pattern),
    font_match: unsafe extern "C" fn(*mut c_void, Pattern, *mut c_int) -> Pattern,
    get_string: unsafe extern "C" fn(Pattern, *const c_char, c_int, *mut *mut u8) -> c_int,
    get_bool: unsafe extern "C" fn(Pattern, *const c_char, c_int, *mut c_int) -> c_int,
    get_integer: unsafe extern "C" fn(Pattern, *const c_char, c_int, *mut c_int) -> c_int,
    destroy: unsafe extern "C" fn(Pattern),
}

const RESULT_MATCH: c_int = 0;
const MATCH_PATTERN: c_int = 0;

impl Library {
    fn load() -> Option<Self> {
        // SAFETY: libfontconfig runs no initialisation code with side effects
        // beyond its own state when loaded.
        let library = unsafe { libloading::Library::new("libfontconfig.so.1") }.ok()?;
        // SAFETY: each symbol is looked up with the C signature fontconfig
        // documents for it (fontconfig.h, stable since 2.0).
        unsafe {
            Some(Self {
                name_parse: *library.get(b"FcNameParse\0").ok()?,
                config_substitute: *library.get(b"FcConfigSubstitute\0").ok()?,
                default_substitute: *library.get(b"FcDefaultSubstitute\0").ok()?,
                font_match: *library.get(b"FcFontMatch\0").ok()?,
                get_string: *library.get(b"FcPatternGetString\0").ok()?,
                get_bool: *library.get(b"FcPatternGetBool\0").ok()?,
                get_integer: *library.get(b"FcPatternGetInteger\0").ok()?,
                destroy: *library.get(b"FcPatternDestroy\0").ok()?,
                _library: library,
            })
        }
    }

    fn query(&self, pattern: &str) -> Option<FontMatch> {
        let pattern = CString::new(pattern).ok()?;
        // SAFETY: `pattern` is a NUL-terminated string; the parsed and the
        // matched patterns are owned here and destroyed once, after their
        // values were copied out. A null configuration means fontconfig's
        // current one, which it initialises on first use and which is safe to
        // use from several threads.
        unsafe {
            let parsed = (self.name_parse)(pattern.as_ptr().cast());
            if parsed.is_null() {
                return None;
            }
            let _ = (self.config_substitute)(core::ptr::null_mut(), parsed, MATCH_PATTERN);
            (self.default_substitute)(parsed);
            let mut result = 0;
            let matched = (self.font_match)(core::ptr::null_mut(), parsed, &mut result);
            (self.destroy)(parsed);
            if matched.is_null() {
                return None;
            }
            let found = self.read(matched);
            (self.destroy)(matched);
            found
        }
    }

    /// Copies the values out of a matched pattern.
    ///
    /// # Safety
    /// `matched` must be a live pattern.
    unsafe fn read(&self, matched: Pattern) -> Option<FontMatch> {
        let mut family: *mut u8 = core::ptr::null_mut();
        // SAFETY: the caller guarantees `matched` is live; the object names
        // are NUL-terminated, and a returned string lives as long as the
        // pattern, so it is copied before the pattern goes.
        unsafe {
            if (self.get_string)(matched, c"family".as_ptr(), 0, &mut family) != RESULT_MATCH
                || family.is_null()
            {
                return None;
            }
            let family = CStr::from_ptr(family.cast()).to_str().ok()?.to_owned();
            let boolean = |object: &CStr| {
                let mut value = 0;
                ((self.get_bool)(matched, object.as_ptr(), 0, &mut value) == RESULT_MATCH)
                    .then_some(value != 0)
            };
            let mut hint_style = 0;
            let hint_style =
                ((self.get_integer)(matched, c"hintstyle".as_ptr(), 0, &mut hint_style)
                    == RESULT_MATCH)
                    .then_some(hint_style);
            Some(FontMatch {
                family,
                antialias: boolean(c"antialias"),
                hinting: boolean(c"hinting"),
                hint_style,
            })
        }
    }
}

// SAFETY: the library holds only function pointers into a loaded shared
// object that is never unloaded while the process runs, and fontconfig's
// matching functions are thread-safe.
unsafe impl Send for Library {}
// SAFETY: as above.
unsafe impl Sync for Library {}

fn library() -> Option<&'static Library> {
    static LIBRARY: OnceLock<Option<Library>> = OnceLock::new();
    LIBRARY.get_or_init(Library::load).as_ref()
}

/// What fontconfig matches for `pattern` (for example `"sans-serif"` or
/// `"sans-serif:lang=ar"`), or `None` without fontconfig.
pub fn match_pattern(pattern: &str) -> Option<FontMatch> {
    library()?.query(pattern)
}

/// The fontconfig language that best stands for `script`, given the
/// text's `locale` for scripts several languages share.
pub fn script_language(script: Script, locale: &str) -> Option<&'static str> {
    let han = || match locale {
        l if l.starts_with("ja") => "ja",
        l if l.starts_with("ko") => "ko",
        l if l.starts_with("zh-HK") || l.starts_with("zh-hk") => "zh-hk",
        l if l.starts_with("zh-TW") || l.starts_with("zh-tw") || l.contains("Hant") => "zh-tw",
        _ => "zh-cn",
    };
    Some(match script {
        Script::Arabic => "ar",
        Script::Armenian => "hy",
        Script::Bengali => "bn",
        Script::Bopomofo | Script::Han => han(),
        Script::Cherokee => "chr",
        Script::Cyrillic => "ru",
        Script::Devanagari => "hi",
        Script::Ethiopic => "am",
        Script::Georgian => "ka",
        Script::Greek => "el",
        Script::Gujarati => "gu",
        Script::Gurmukhi => "pa",
        Script::Hangul => "ko",
        Script::Hebrew => "he",
        Script::Hiragana | Script::Katakana => "ja",
        Script::Kannada => "kn",
        Script::Khmer => "km",
        Script::Lao => "lo",
        Script::Malayalam => "ml",
        Script::Mongolian => "mn",
        Script::Myanmar => "my",
        Script::Oriya => "or",
        Script::Sinhala => "si",
        Script::Syriac => "syr",
        Script::Tamil => "ta",
        Script::Telugu => "te",
        Script::Thaana => "dv",
        Script::Thai => "th",
        Script::Tibetan => "bo",
        Script::Yi => "ii",
        _ => return None,
    })
}

/// fontconfig's family for `script`, followed by `built_in`, cached per
/// script and locale. `None` when fontconfig has no answer for the script.
pub(crate) fn script_fallback(
    script: Script,
    locale: &str,
    built_in: &'static [&'static str],
) -> Option<&'static [&'static str]> {
    type Cache = HashMap<(Script, String), Option<&'static [&'static str]>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let language = script_language(script, locale)?;
    let cache = CACHE.get_or_init(Mutex::default);
    let key = (script, locale.to_owned());
    if let Some(list) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&key)
    {
        return *list;
    }
    let list = match_pattern(&alloc::format!("sans-serif:lang={language}")).map(|found| {
        let family: &'static str = Box::leak(found.family.into_boxed_str());
        let mut list: Vec<&'static str> = Vec::with_capacity(built_in.len() + 1);
        list.push(family);
        list.extend(built_in.iter().filter(|name| **name != family));
        let list: &'static [&'static str] = Box::leak(list.into_boxed_slice());
        list
    });
    // One leaked list per script and locale, at most.
    let _ = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, list);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    /// fontconfig's answer, as the `fc-match` tool gives it.
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

    #[test]
    fn matches_agree_with_fc_match() {
        for pattern in ["sans-serif", "monospace", "sans-serif:lang=ar"] {
            let Some(expected) = fc_match(pattern) else {
                // No fontconfig tools on this host.
                return;
            };
            let found = match_pattern(pattern).expect("fontconfig is installed");
            assert_eq!(found.family, expected, "{pattern}");
        }
    }

    #[test]
    fn scripts_map_to_languages() {
        assert_eq!(script_language(Script::Arabic, "en-US"), Some("ar"));
        assert_eq!(script_language(Script::Han, "ja-JP"), Some("ja"));
        assert_eq!(script_language(Script::Han, "zh-TW"), Some("zh-tw"));
        assert_eq!(script_language(Script::Han, "en-US"), Some("zh-cn"));
        assert_eq!(script_language(Script::Latin, "en-US"), None);
    }

    #[test]
    fn the_fontconfig_family_leads_the_script_fallback() {
        let Some(expected) = fc_match("sans-serif:lang=ar") else {
            return;
        };
        let list = script_fallback(Script::Arabic, "en-US", &["Noto Sans Arabic"])
            .expect("fontconfig is installed");
        assert_eq!(list[0], expected);
        assert!(list.contains(&"Noto Sans Arabic"));
    }
}
