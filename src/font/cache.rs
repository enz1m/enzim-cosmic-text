// SPDX-License-Identifier: MIT OR Apache-2.0

//! Persistent on-disk cache for the parsed system-font index.
//!
//! [`fontdb::Database::load_system_fonts`] walks every system font directory and
//! `mmap`s + parses each font file to populate per-face metadata. The result lives only
//! in memory and is rebuilt from scratch on every process start.
//!
//! This module persists the small, parsed [`fontdb::FaceInfo`] set to disk and reloads it
//! via [`fontdb::Database::push_face_info`] instead of re-parsing every font file. It is a
//! pure-Rust, dependency-free reimplementation of what fontconfig's `fc-cache` does.
//!
//! The cache is keyed on the font search environment and a cheap fingerprint of the
//! fontconfig files, every directory under the font roots (taken before the scan, so a
//! change during the scan invalidates it), and each font file's modification time and
//! size. On any mismatch the normal scan runs and the cache is rewritten atomically.

use crate::HashMap;
use alloc::string::String;
use alloc::vec::Vec;
use fontdb::{Database, FaceInfo, Source, Stretch, Style, Weight, ID};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Magic bytes identifying a cosmic-text font cache file (`C`osmic `T`ext `F`ont `C`ache).
const MAGIC: [u8; 4] = *b"CTFC";

/// Cache format version. Bump whenever the on-disk layout changes so that stale caches
/// written by an older version are rejected instead of misparsed.
const VERSION: u32 = 2;

/// Environment variables that select the fontconfig files and font roots.
const ENVIRONMENT: [&str; 4] = [
    "FONTCONFIG_FILE",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "HOME",
];

const GENERIC_FAMILIES: [fontdb::Family<'static>; 5] = [
    fontdb::Family::Serif,
    fontdb::Family::SansSerif,
    fontdb::Family::Cursive,
    fontdb::Family::Fantasy,
    fontdb::Family::Monospace,
];

type Mtime = (u64, u32);

struct Cached {
    generic_families: Vec<String>,
    faces: Vec<FaceInfo>,
}

/// Loads the system fonts into `db`, using the on-disk cache at `cache_path` when it is
/// present and still valid.
///
/// On a cache hit no font file is parsed: the stored [`FaceInfo`] entries are inserted
/// directly with [`Database::push_face_info`]. On a miss (or an invalidated cache) the
/// normal [`Database::load_system_fonts`] scan runs and its result is written back to the
/// cache for the next start.
pub(crate) fn load_system_fonts_cached(db: &mut Database, cache_path: &Path) {
    let environment = environment();
    match try_load(cache_path, &environment) {
        Some(cached) => {
            log::debug!(
                "Loaded {} font faces from cache '{}'.",
                cached.faces.len(),
                cache_path.display()
            );
            for face in cached.faces {
                db.push_face_info(face);
            }
            for (family, name) in GENERIC_FAMILIES.iter().zip(cached.generic_families) {
                set_generic_family(db, family, name);
            }
        }
        None => {
            let watched = font_roots().map(|(config_paths, roots)| watch(config_paths, &roots));
            db.load_system_fonts();
            if let Err(err) = write(db, cache_path, &environment, watched) {
                log::warn!(
                    "Failed to write font cache '{}': {err}",
                    cache_path.display()
                );
            }
        }
    }
}

/// Returns the default cache file path (`<cache-dir>/cosmic-text/fonts.cache`), following
/// the platform's conventional cache directory. Returns `None` if no cache directory can
/// be determined from the environment.
pub(crate) fn default_cache_path() -> Option<PathBuf> {
    let mut dir = cache_dir()?;
    dir.push("cosmic-text");
    dir.push("fonts.cache");
    Some(dir)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn cache_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Some(path);
        }
    }
    let mut home = PathBuf::from(std::env::var_os("HOME")?);
    home.push(".cache");
    Some(home)
}

#[cfg(target_os = "macos")]
fn cache_dir() -> Option<PathBuf> {
    let mut home = PathBuf::from(std::env::var_os("HOME")?);
    home.push("Library");
    home.push("Caches");
    Some(home)
}

#[cfg(windows)]
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

#[cfg(not(any(unix, windows)))]
fn cache_dir() -> Option<PathBuf> {
    None
}

//TODO: Redox?!

/// Reads `cache_path` and, if it is well-formed and its fingerprint still matches the
/// filesystem, returns the cached set of faces. Returns `None` on any error, version
/// mismatch, or fingerprint mismatch.
fn try_load(cache_path: &Path, environment: &[Option<String>]) -> Option<Cached> {
    let bytes = std::fs::read(cache_path).ok()?;
    let mut r = Reader::new(&bytes);

    if r.take(4)? != MAGIC {
        return None;
    }
    if r.u32()? != VERSION {
        return None;
    }

    for value in environment {
        if r.optional_string()? != *value {
            return None;
        }
    }

    let watched_count = r.u32()?;
    for _ in 0..watched_count {
        let path = r.path()?;
        let mtime = r.optional_mtime()?;
        if path_mtime(&path) != mtime {
            return None;
        }
    }

    let generic_families = GENERIC_FAMILIES
        .iter()
        .map(|_| r.string())
        .collect::<Option<Vec<_>>>()?;

    // File fingerprint
    let file_count = r.u32()? as usize;
    let mut files = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        let path = r.path()?;
        let mtime = r.mtime()?;
        let len = r.u64()?;
        match file_fingerprint(&path) {
            Some((m, l)) if m == mtime && l == len => {}
            _ => return None,
        }
        files.push(path);
    }

    // Face table. Each face references a file by index into `files`.
    let face_count = r.u32()? as usize;
    let mut faces = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        let file_index = r.u32()? as usize;
        let source_path = files.get(file_index)?.clone();
        let index = r.u32()?;

        let family_count = r.u32()? as usize;
        let mut families = Vec::with_capacity(family_count);
        for _ in 0..family_count {
            // TODO: Language is not persisted, but it is also not used
            families.push((r.string()?, fontdb::Language::Unknown));
        }

        let post_script_name = r.string()?;
        let style = decode_style(r.u8()?)?;
        let weight = Weight(r.u16()?);
        let stretch = decode_stretch(r.u8()?)?;
        let monospaced = r.u8()? != 0;

        faces.push(FaceInfo {
            id: ID::dummy(),
            source: Source::File(source_path),
            index,
            families,
            post_script_name,
            style,
            weight,
            stretch,
            monospaced,
        });
    }

    Some(Cached {
        generic_families,
        faces,
    })
}

/// Serializes the file-backed faces in `db` to `cache_path`, writing atomically
fn write(
    db: &Database,
    cache_path: &Path,
    environment: &[Option<String>],
    watched: Option<Vec<(PathBuf, Option<Mtime>)>>,
) -> std::io::Result<()> {
    let mut file_indices: HashMap<PathBuf, u32> = HashMap::default();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut faces: Vec<(u32, &FaceInfo)> = Vec::new();

    for face in db.faces() {
        // cosmic-text's `std` feature always enables `fontdb/memmap`
        let path = match &face.source {
            Source::File(path) => path,
            Source::SharedFile(path, _) => path,
            // Binary sources (e.g. user-provided fonts) are not persistable.
            Source::Binary(_) => continue,
        };
        let index = *file_indices.entry(path.clone()).or_insert_with(|| {
            let i = files.len() as u32;
            files.push(path.clone());
            i
        });
        faces.push((index, face));
    }

    // Without known font roots, the directories that directly contained a scanned font.
    let watched = watched.unwrap_or_else(|| {
        let mut seen_dirs: HashSet<&Path> = HashSet::default();
        files
            .iter()
            .filter_map(|path| path.parent())
            .filter(|dir| seen_dirs.insert(dir))
            .map(|dir| (dir.to_path_buf(), path_mtime(dir)))
            .collect()
    });

    let mut w = Writer::new();
    w.bytes(&MAGIC);
    w.u32(VERSION);

    for value in environment {
        w.optional_string(value.as_deref());
    }

    w.u32(watched.len() as u32);
    for (path, mtime) in &watched {
        w.path(path);
        w.optional_mtime(*mtime);
    }

    for family in &GENERIC_FAMILIES {
        w.string(db.family_name(family));
    }

    w.u32(files.len() as u32);
    for path in &files {
        w.path(path);
        let (mtime, len) = file_fingerprint(path).unwrap_or(((0, 0), 0));
        w.mtime(mtime);
        w.u64(len);
    }

    w.u32(faces.len() as u32);
    for (file_index, face) in &faces {
        w.u32(*file_index);
        w.u32(face.index);
        w.u32(face.families.len() as u32);
        for (name, _language) in &face.families {
            w.string(name);
        }
        w.string(&face.post_script_name);
        w.u8(encode_style(face.style));
        w.u16(face.weight.0);
        w.u8(face.stretch.to_number() as u8);
        w.u8(u8::from(face.monospaced));
    }

    write_atomic(cache_path, &w.into_inner())
}

/// Writes `data` to `path` atomically
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut tmp = path.as_os_str().to_os_string();
    // A per-process suffix keeps concurrent writers from writing to eachother's temp file
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);

    std::fs::write(&tmp, data)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            Err(err)
        }
    }
}

fn path_mtime(path: &Path) -> Option<Mtime> {
    mtime_parts(std::fs::metadata(path).ok()?.modified().ok()?)
}

fn environment() -> Vec<Option<String>> {
    ENVIRONMENT
        .iter()
        .map(|name| std::env::var(name).ok())
        .collect()
}

fn set_generic_family(db: &mut Database, family: &fontdb::Family<'_>, name: String) {
    match family {
        fontdb::Family::Serif => db.set_serif_family(name),
        fontdb::Family::SansSerif => db.set_sans_serif_family(name),
        fontdb::Family::Cursive => db.set_cursive_family(name),
        fontdb::Family::Fantasy => db.set_fantasy_family(name),
        fontdb::Family::Monospace => db.set_monospace_family(name),
        fontdb::Family::Name(_) => {}
    }
}

/// Fingerprints the configuration files, each font root, and every directory below it.
/// Missing paths are recorded too, so that creating one invalidates the cache.
fn watch(config_paths: Vec<PathBuf>, roots: &[PathBuf]) -> Vec<(PathBuf, Option<Mtime>)> {
    let mut watched: Vec<_> = config_paths
        .into_iter()
        .map(|path| {
            let mtime = path_mtime(&path);
            (path, mtime)
        })
        .collect();
    let mut seen = HashSet::new();
    for root in roots {
        watch_dir(root.clone(), &mut seen, &mut watched);
    }
    watched
}

fn watch_dir(
    dir: PathBuf,
    seen: &mut HashSet<PathBuf>,
    watched: &mut Vec<(PathBuf, Option<Mtime>)>,
) {
    let mtime = path_mtime(&dir);
    let entries = std::fs::read_dir(&dir);
    watched.push((dir, mtime));
    for entry in entries.into_iter().flatten().flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        let is_dir = file_type.is_dir()
            || (file_type.is_symlink() && std::fs::metadata(&path).is_ok_and(|m| m.is_dir()));
        if is_dir {
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if seen.insert(path.clone()) {
                watch_dir(path, seen, watched);
            }
        }
    }
}

/// The configuration files and font roots that [`Database::load_system_fonts`] reads, where
/// they are known.
#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
fn font_roots() -> Option<(Vec<PathBuf>, Vec<PathBuf>)> {
    let home = std::env::var_os("HOME").map(PathBuf::from);

    #[cfg(feature = "fontconfig")]
    let (config_paths, roots) = fontconfig_roots(home.as_deref());
    #[cfg(not(feature = "fontconfig"))]
    let (config_paths, roots) = (Vec::new(), Vec::new());

    if !roots.is_empty() {
        return Some((config_paths, roots));
    }
    let mut roots = vec![
        PathBuf::from("/usr/share/fonts/"),
        PathBuf::from("/usr/local/share/fonts/"),
    ];
    if let Some(home) = home {
        roots.push(home.join(".fonts"));
        roots.push(home.join(".local/share/fonts"));
    }
    Some((config_paths, roots))
}

#[cfg(not(all(
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
)))]
fn font_roots() -> Option<(Vec<PathBuf>, Vec<PathBuf>)> {
    None
}

/// Mirrors fontdb's fontconfig lookup. No roots means fontdb scans its fixed directories.
#[cfg(all(
    feature = "fontconfig",
    unix,
    not(any(target_os = "macos", target_os = "ios", target_os = "android"))
))]
fn fontconfig_roots(home: Option<&Path>) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut config = fontconfig_parser::FontConfig::default();
    let mut config_paths = Vec::new();

    if let Some(file) = std::env::var_os("FONTCONFIG_FILE") {
        config_paths.push(PathBuf::from(&file));
        let _ = config.merge_config(Path::new(&file));
    } else {
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|home| home.join(".config")));
        let read_global = match config_home {
            Some(config_home) => {
                let user = config_home.join("fontconfig/fonts.conf");
                config_paths.push(config_home.join("fontconfig/conf.d"));
                config_paths.push(user.clone());
                config.merge_config(&user).is_err()
            }
            None => true,
        };
        if read_global {
            config_paths.push(PathBuf::from("/etc/fonts/local.conf"));
            let _ = config.merge_config(Path::new("/etc/fonts/local.conf"));
        }
        config_paths.push(PathBuf::from("/etc/fonts/fonts.conf"));
        let _ = config.merge_config(Path::new("/etc/fonts/fonts.conf"));
    }
    // Package-installed configuration is linked into the conventional include directory.
    config_paths.push(PathBuf::from("/etc/fonts/conf.d"));
    config_paths.extend(config.config_files);

    let roots = config
        .dirs
        .into_iter()
        .filter_map(|dir| match dir.path.strip_prefix("~") {
            Ok(relative) => home.map(|home| home.join(relative)),
            Err(_) => Some(dir.path),
        })
        .collect();
    (config_paths, roots)
}

/// Returns `((secs, nanos), len)` for a regular file, or `None` if it is missing or not a file.
fn file_fingerprint(path: &Path) -> Option<((u64, u32), u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some((mtime_parts(metadata.modified().ok()?)?, metadata.len()))
}

fn mtime_parts(time: SystemTime) -> Option<(u64, u32)> {
    let dur = time.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    Some((dur.as_secs(), dur.subsec_nanos()))
}

fn encode_style(style: Style) -> u8 {
    match style {
        Style::Normal => 0,
        Style::Italic => 1,
        Style::Oblique => 2,
    }
}

fn decode_style(value: u8) -> Option<Style> {
    match value {
        0 => Some(Style::Normal),
        1 => Some(Style::Italic),
        2 => Some(Style::Oblique),
        _ => None,
    }
}

fn decode_stretch(value: u8) -> Option<Stretch> {
    Some(match value {
        1 => Stretch::UltraCondensed,
        2 => Stretch::ExtraCondensed,
        3 => Stretch::Condensed,
        4 => Stretch::SemiCondensed,
        5 => Stretch::Normal,
        6 => Stretch::SemiExpanded,
        7 => Stretch::Expanded,
        8 => Stretch::ExtraExpanded,
        9 => Stretch::UltraExpanded,
        _ => return None,
    })
}

/// Append-only little-endian byte writer.
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    fn bytes(&mut self, value: &[u8]) {
        self.buf.extend_from_slice(value);
    }

    fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn mtime(&mut self, (secs, nanos): (u64, u32)) {
        self.u64(secs);
        self.u32(nanos);
    }

    fn string(&mut self, value: &str) {
        self.u32(value.len() as u32);
        self.bytes(value.as_bytes());
    }

    /// Encodes a path as its lossy UTF-8 string form
    fn path(&mut self, path: &Path) {
        self.string(&path.to_string_lossy());
    }

    fn optional_string(&mut self, value: Option<&str>) {
        self.u8(u8::from(value.is_some()));
        if let Some(value) = value {
            self.string(value);
        }
    }

    fn optional_mtime(&mut self, value: Option<Mtime>) {
        self.u8(u8::from(value.is_some()));
        if let Some(value) = value {
            self.mtime(value);
        }
    }
}

/// Bounds-checked little-endian byte reader. Every accessor returns `None` on truncation
/// or malformed data so a corrupt cache is rejected rather than panicking.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn mtime(&mut self) -> Option<(u64, u32)> {
        Some((self.u64()?, self.u32()?))
    }

    fn string(&mut self) -> Option<String> {
        let len = self.u32()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).ok()
    }

    fn path(&mut self) -> Option<PathBuf> {
        Some(PathBuf::from(self.string()?))
    }

    fn optional_string(&mut self) -> Option<Option<String>> {
        match self.u8()? {
            0 => Some(None),
            1 => Some(Some(self.string()?)),
            _ => None,
        }
    }

    fn optional_mtime(&mut self) -> Option<Option<Mtime>> {
        match self.u8()? {
            0 => Some(None),
            1 => Some(Some(self.mtime()?)),
            _ => None,
        }
    }
}
