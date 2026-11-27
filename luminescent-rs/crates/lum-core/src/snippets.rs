//! Snippets' data side: what a file in the snippets folder is, the order, favourites and folder colours kept in
//! `<snippets folder>/.luminescent.json`, searching every folder, moving clips about and turning the clipboard into
//! files. Nothing here draws or touches the clipboard; `lum-ui` does that.
//!
//! The meta file is kept over raw JSON so a key this version does not know survives a save. Paths inside it are
//! relative to the snippets folder and use the system's separator, as Python's `os.path.relpath` writes them.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, MAIN_SEPARATOR, Path, PathBuf};
use std::time::SystemTime;

use jiff::civil::DateTime;
use jiff::{Timestamp, ZonedDateTime, tz::TimeZone};
use serde_json::{Map, Value, json};

use crate::pyjson::{self, Style};
use crate::store::{self, StoreError};

pub const TEXT_EXT: [&str; 3] = ["txt", "md", "py"];
pub const IMAGE_EXT: [&str; 6] = ["jpg", "jpeg", "png", "gif", "webp", "bmp"];
pub const VIDEO_EXT: [&str; 5] = ["mp4", "mov", "avi", "webm", "mkv"];

/// How many matches a search lists before it stops looking.
pub const SEARCH_LIMIT: usize = 200;

/// How much of a text file the card and the colour and link checks read.
const CARD_CHARS: usize = 600;
const SEARCH_CHARS: usize = 20_000;

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

pub fn is_text(path: &Path) -> bool {
    TEXT_EXT.contains(&extension(path).as_str())
}

/// What a card shows for a file.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Image,
    Video,
    /// A one-line `http` address: the title is the file's name without its extension, the body the address
    /// without the scheme.
    Link {
        title: String,
        url: String,
    },
    /// A text file that holds one `#rgb` or `#rrggbb` colour.
    Colour(String),
    Code(String),
    Text(String),
    /// Anything else, with its name cut to the name limit.
    File(String),
}

impl Kind {
    /// The word on the card's footer.
    pub fn label(&self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Link { .. } => "link",
            Kind::Colour(_) => "colour",
            Kind::Code(_) => "code",
            Kind::Text(_) => "text",
            Kind::File(_) => "file",
        }
    }

    /// Cards that get the bigger look when the mouse rests on them (a file shows its name in a tooltip).
    pub fn has_preview(&self) -> bool {
        !matches!(self, Kind::File(_))
    }
}

/// Reads up to `chars` characters of a text file. Bytes that are not UTF-8 are dropped, as Python's
/// `errors="ignore"` does, and every line ending becomes `\n`.
pub fn read_chars(path: &Path, chars: usize) -> io::Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(chars as u64 * 4).read_to_end(&mut bytes)?;
    Ok(decode(&bytes).chars().take(chars).collect())
}

/// A whole text file, read the same way.
pub fn read_text(path: &Path) -> io::Result<String> {
    Ok(decode(&fs::read(path)?))
}

fn decode(bytes: &[u8]) -> String {
    let text: String = String::from_utf8_lossy(bytes).chars().filter(|c| *c != '\u{fffd}').collect();
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn shortened(name: &str, limit: usize) -> String {
    if name.chars().count() <= limit {
        name.to_owned()
    } else {
        format!("{}…", name.chars().take(limit).collect::<String>())
    }
}

fn is_colour(text: &str) -> bool {
    let Some(hex) = text.strip_prefix('#') else { return false };
    matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// What a file is, from its extension and, for text, its first 600 characters.
pub fn classify(path: &Path, name_limit: usize) -> Kind {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let short = shortened(&name, name_limit);
    let ext = extension(path);
    if IMAGE_EXT.contains(&ext.as_str()) {
        return Kind::Image;
    }
    if VIDEO_EXT.contains(&ext.as_str()) {
        return Kind::Video;
    }
    if !TEXT_EXT.contains(&ext.as_str()) {
        return Kind::File(short);
    }
    let Ok(content) = read_chars(path, CARD_CHARS) else { return Kind::File(short) };
    let content = content.trim().to_owned();
    let one_line = !content.contains('\n');
    if one_line && (content.starts_with("http://") || content.starts_with("https://")) {
        let title = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let url = content.split_once("://").map(|(_, rest)| rest.to_owned()).unwrap_or_default();
        return Kind::Link { title, url };
    }
    if one_line && matches!(content.chars().count(), 4 | 7) && is_colour(&content) {
        return Kind::Colour(content);
    }
    if ext == "py" {
        return Kind::Code(content.chars().take(400).collect());
    }
    let body: String = content.chars().take(300).collect();
    Kind::Text(if body.is_empty() { short } else { body })
}

/// How long ago a file changed, the short way: now, 2m, 3h, yesterday, 4d, 12 Oct.
pub fn age_text(modified: &ZonedDateTime, now: &ZonedDateTime) -> String {
    // the PC clock can jump ahead: never negative
    let secs = (now.timestamp().as_second() - modified.timestamp().as_second()).max(0);
    match secs {
        0..60 => "now".to_owned(),
        60..3600 => format!("{}m", secs / 60),
        3600..86_400 => format!("{}h", secs / 3600),
        86_400..172_800 => "yesterday".to_owned(),
        172_800..604_800 => format!("{}d", secs / 86_400),
        _ => modified.strftime("%d %b").to_string().trim_start_matches('0').to_owned(),
    }
}

/// The age of a file on disk, or empty when it can not be read.
pub fn file_age(path: &Path, now: &ZonedDateTime) -> String {
    let Some(modified) = fs::metadata(path).ok().and_then(|m| m.modified().ok()) else {
        return String::new();
    };
    let Ok(stamp) = Timestamp::try_from(modified) else { return String::new() };
    age_text(&stamp.to_zoned_datetime(TimeZone::system()), now)
}

/// A size the short way: 800 B, 12 KB, 3.4 MB.
pub fn size_text(bytes: u64) -> String {
    let mut n = bytes as f64;
    for unit in ["B", "KB", "MB", "GB"] {
        if n < 1024.0 || unit == "GB" {
            return if unit == "B" || unit == "KB" {
                format!("{n:.0} {unit}")
            } else {
                format!("{n:.1} {unit}")
            };
        }
        n /= 1024.0;
    }
    String::new()
}

/// `{date}`, `{time}`, `{datetime}`, `{weekday}` and `{clipboard}` in a text snippet. Anything else in braces is
/// left alone, so code snippets survive.
pub fn fill_placeholders(text: &str, clipboard: &str, now: DateTime) -> String {
    let values = [
        ("{date}", now.strftime("%Y-%m-%d").to_string()),
        ("{time}", now.strftime("%H:%M").to_string()),
        ("{datetime}", now.strftime("%Y-%m-%d %H:%M").to_string()),
        ("{weekday}", now.strftime("%A").to_string()),
        ("{clipboard}", clipboard.to_owned()),
    ];
    let mut text = text.to_owned();
    for (key, value) in values {
        if text.contains(key) {
            text = text.replace(key, &value);
        }
    }
    text
}

/// `name (2).ext`, `name (3).ext`... until nothing is there.
pub fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_owned();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let folder = path.parent().map(Path::to_owned).unwrap_or_default();
    (2..)
        .map(|n| folder.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("an unused name turns up")
}

/// The file name for a text saved from the clipboard: its first line, cut at 40 characters and cleaned up.
pub fn text_clip_name(text: &str, stamp: &str) -> String {
    let first = text.trim().lines().next().unwrap_or_default();
    let first: String = first.chars().take(40).collect();
    let safe: String = first.chars().filter(|c| c.is_alphanumeric() || " -_".contains(*c)).collect();
    let safe = safe.trim();
    format!("{}.txt", if safe.is_empty() { format!("Clip {stamp}") } else { safe.to_owned() })
}

/// Cleans `.` and `..` out of a path without touching the disk (`os.path.normpath`).
pub fn normpath(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() { PathBuf::from(".") } else { out }
}

/// A path as a map key: Windows paths do not care about case.
pub fn norm(path: &Path) -> String {
    let text = normpath(path).to_string_lossy().into_owned();
    if cfg!(windows) { text.to_lowercase().replace('/', "\\") } else { text }
}

pub fn same(a: &Path, b: &Path) -> bool {
    norm(a) == norm(b)
}

fn lower(name: &str) -> String {
    name.to_lowercase()
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn visible_names(folder: &Path) -> io::Result<Vec<String>> {
    Ok(fs::read_dir(folder)?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect())
}

/// How many things a folder holds, hidden ones left out.
pub fn count_in(folder: &Path) -> usize {
    visible_names(folder).map(|n| n.len()).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct FolderEntry {
    pub path: PathBuf,
    pub count: usize,
}

/// One clip with what its card needs, read once when the folder is listed.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    pub path: PathBuf,
    pub kind: Kind,
    /// The folder it sits in, relative to the snippets folder; empty at the top (shown in search results and the
    /// favourites row).
    pub where_: String,
    pub age: String,
    pub size: String,
    pub modified: Option<SystemTime>,
}

impl Clip {
    pub fn read(path: &Path, root: &Path, name_limit: usize, now: &ZonedDateTime) -> Clip {
        let meta = fs::metadata(path).ok();
        let kind = classify(path, name_limit);
        let size = match (&kind, &meta) {
            (Kind::File(_), Some(m)) => size_text(m.len()),
            _ => String::new(),
        };
        let folder = path.parent().unwrap_or(Path::new(""));
        let where_ = match relative(folder, root) {
            rel if rel == "." => String::new(),
            rel => rel,
        };
        Clip {
            path: path.to_owned(),
            kind,
            where_,
            age: file_age(path, now),
            size,
            modified: meta.and_then(|m| m.modified().ok()),
        }
    }
}

/// What the grid shows for one folder, or for a search.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Listing {
    /// Folder colours by normalised path, and every favourite, for the cards to look themselves up in.
    pub colours: HashMap<String, String>,
    pub favourite_paths: Vec<PathBuf>,
    pub favourites: Vec<Clip>,
    pub folders: Vec<FolderEntry>,
    pub clips: Vec<Clip>,
    /// The line that replaces the grid when there is nothing to show, or a note below it.
    pub message: String,
}

/// The folder as the grid shows it: favourites (at the top level), folders by name, clips in the stored order.
pub fn list_folder(meta: &Meta, folder: &Path, name_limit: usize) -> Listing {
    let mut listing =
        Listing { colours: meta.folder_colours(), favourite_paths: meta.favourites(), ..Listing::default() };
    let names = match visible_names(folder) {
        Ok(names) => names,
        Err(e) => {
            listing.message = format!("Cannot open folder:\n{}\n\n{e}", folder.display());
            return listing;
        }
    };
    let now = ZonedDateTime::now();
    let order: HashMap<String, usize> =
        meta.order_of(folder).into_iter().enumerate().map(|(i, n)| (n, i)).collect();
    let (mut dirs, mut files): (Vec<String>, Vec<String>) =
        names.iter().cloned().partition(|n| folder.join(n).is_dir());
    dirs.sort_by_key(|n| lower(n));
    files.sort_by_key(|n| (order.get(n).copied().unwrap_or(order.len()), lower(n)));
    if same(folder, meta.root()) {
        listing.favourites = meta
            .favourites()
            .into_iter()
            .filter(|f| f.is_file())
            .map(|f| Clip::read(&f, meta.root(), name_limit, &now))
            .collect();
    }
    listing.folders =
        dirs.iter().map(|n| FolderEntry { path: folder.join(n), count: count_in(&folder.join(n)) }).collect();
    listing.clips =
        files.iter().map(|n| Clip::read(&folder.join(n), meta.root(), name_limit, &now)).collect();
    if names.is_empty() {
        listing.message = "This folder is empty. Copy something and press Paste here.".to_owned();
    }
    listing
}

/// Every folder under the root: folder names, file names and what text clips say.
pub fn search(meta: &Meta, query: &str, name_limit: usize) -> Listing {
    let query = query.trim().to_lowercase();
    let root = meta.root();
    let (mut found_dirs, mut found) = (Vec::new(), Vec::new());
    let mut pending = vec![root.to_owned()];
    while let Some(dir) = pending.pop() {
        let Ok(names) = visible_names(&dir) else { continue };
        let (mut dirs, mut files): (Vec<String>, Vec<String>) =
            names.into_iter().partition(|n| dir.join(n).is_dir());
        dirs.sort();
        files.sort_by_key(|n| lower(n));
        for d in &dirs {
            if lower(d).contains(&query) {
                found_dirs.push(dir.join(d));
            }
        }
        for n in &files {
            let full = dir.join(n);
            let hit = lower(n).contains(&query)
                || (is_text(&full)
                    && read_chars(&full, SEARCH_CHARS)
                        .is_ok_and(|text| text.to_lowercase().contains(&query)));
            if hit {
                found.push(full);
            }
        }
        if found.len() >= SEARCH_LIMIT {
            break;
        }
        // depth first in name order, as os.walk goes
        pending.extend(dirs.iter().rev().map(|d| dir.join(d)));
    }
    let now = ZonedDateTime::now();
    let mut listing = Listing {
        colours: meta.folder_colours(),
        favourite_paths: meta.favourites(),
        folders: found_dirs
            .iter()
            .take(SEARCH_LIMIT)
            .map(|p| FolderEntry { path: p.clone(), count: count_in(p) })
            .collect(),
        clips: found.iter().take(SEARCH_LIMIT).map(|p| Clip::read(p, root, name_limit, &now)).collect(),
        ..Listing::default()
    };
    if found.is_empty() && found_dirs.is_empty() {
        listing.message = format!("Nothing in any folder matches \"{query}\".");
    } else if found.len() >= SEARCH_LIMIT {
        listing.message = format!("Showing the first {SEARCH_LIMIT} matches.");
    }
    listing
}

/// `path` relative to `base` with the system's separator, `.` for the folder itself.
pub fn relative(path: &Path, base: &Path) -> String {
    let (path, base) = (normpath(path), normpath(base));
    match path.strip_prefix(&base) {
        Ok(rest) if rest.as_os_str().is_empty() => ".".to_owned(),
        Ok(rest) => rest.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// The meta file of a snippets folder.
#[derive(Debug, Clone)]
pub struct Meta {
    root: PathBuf,
    file: PathBuf,
    data: Map<String, Value>,
}

impl Meta {
    pub fn load(root: &Path) -> Meta {
        let root = normpath(root);
        let file = root.join(".luminescent.json");
        let data = match store::load_json(&file) {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };
        Meta { root, file, data }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn save(&self) -> Result<(), StoreError> {
        store::save_text(&self.file, &self.text())
    }

    /// Where the file is and what to write into it, for a background writer.
    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn text(&self) -> String {
        pyjson::to_string(&Value::Object(self.data.clone()), Style::indented(2))
    }

    fn rel(&self, path: &Path) -> String {
        relative(path, &self.root)
    }

    fn abs(&self, rel: &str) -> PathBuf {
        normpath(&self.root.join(rel))
    }

    fn strings(&self, key: &str) -> Vec<String> {
        match self.data.get(key) {
            Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
            _ => Vec::new(),
        }
    }

    /// Folder colours by the folder's normalised path.
    pub fn folder_colours(&self) -> HashMap<String, String> {
        let Some(Value::Object(colours)) = self.data.get("folder_colours") else { return HashMap::new() };
        colours.iter().filter_map(|(k, v)| v.as_str().map(|c| (norm(&self.abs(k)), c.to_owned()))).collect()
    }

    pub fn folder_colour(&self, folder: &Path) -> Option<String> {
        self.folder_colours().remove(&norm(folder))
    }

    pub fn set_folder_colour(&mut self, folder: &Path, colour: &str) {
        let key = self.rel(folder);
        let colours = self.object_mut("folder_colours");
        colours.insert(key, json!(colour));
    }

    fn object_mut(&mut self, key: &str) -> &mut Map<String, Value> {
        if !self.data.get(key).is_some_and(Value::is_object) {
            self.data.insert(key.to_owned(), json!({}));
        }
        self.data.get_mut(key).and_then(Value::as_object_mut).expect("the key was just made an object")
    }

    pub fn favourites(&self) -> Vec<PathBuf> {
        self.strings("favourites").iter().map(|f| self.abs(f)).collect()
    }

    pub fn is_favourite(&self, path: &Path) -> bool {
        self.favourites().iter().any(|f| same(f, path))
    }

    /// Puts `path` in the favourites row, just before `before` when that is in it, else last; or takes it out.
    pub fn set_favourite(&mut self, path: &Path, on: bool, before: Option<&Path>) {
        let me = self.rel(path);
        let mut favs: Vec<String> = self.strings("favourites").into_iter().filter(|f| *f != me).collect();
        if on {
            let at = before
                .map(|b| self.rel(b))
                .and_then(|b| favs.iter().position(|f| *f == b))
                .unwrap_or(favs.len());
            favs.insert(at, me);
        }
        self.data.insert("favourites".into(), json!(favs));
    }

    /// The key a folder's order is kept under. The top level is `.` as Python writes it; a file written by hand
    /// with an empty name for it keeps that name.
    fn order_key(&self, folder: &Path) -> String {
        let key = self.rel(folder);
        let has = |k: &str| matches!(self.data.get("order"), Some(Value::Object(o)) if o.contains_key(k));
        if key == "." && !has(".") && has("") { String::new() } else { key }
    }

    /// The names kept in order for a folder.
    pub fn order_of(&self, folder: &Path) -> Vec<String> {
        let key = self.order_key(folder);
        match self.data.get("order") {
            Some(Value::Object(order)) => order
                .get(&key)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// Puts clip `path` just before `before` in their folder's order.
    pub fn place_before(&mut self, path: &Path, before: &Path) {
        let Some(folder) = before.parent() else { return };
        if same(path, before) {
            return;
        }
        let Ok(names) = visible_names(folder) else { return };
        let mut names: Vec<String> = names.into_iter().filter(|n| !folder.join(n).is_dir()).collect();
        let known: HashMap<String, usize> =
            self.order_of(folder).into_iter().enumerate().map(|(i, n)| (n, i)).collect();
        names.sort_by_key(|n| (known.get(n).copied().unwrap_or(known.len()), lower(n)));
        let (me, target) = (name_of(path), name_of(before));
        names.retain(|n| *n != me);
        let at = names.iter().position(|n| *n == target).unwrap_or(names.len());
        names.insert(at, me);
        let key = self.order_key(folder);
        self.object_mut("order").insert(key, json!(names));
    }

    /// A clip or folder was moved or renamed: it keeps its favourite, colour and place.
    pub fn path_moved(&mut self, old: &Path, new: &Path) {
        let (ro, rn) = (self.rel(old), self.rel(new));
        let under = |p: &str| p == ro || p.starts_with(&format!("{ro}{MAIN_SEPARATOR}"));
        let moved = |p: &str| format!("{rn}{}", &p[ro.len()..]);
        if let Some(Value::Array(favs)) = self.data.get_mut("favourites") {
            for f in favs.iter_mut() {
                if let Some(text) = f.as_str().filter(|t| under(t)) {
                    *f = json!(moved(text));
                }
            }
        }
        if let Some(Value::Object(colours)) = self.data.get_mut("folder_colours") {
            let renamed: Map<String, Value> = std::mem::take(colours)
                .into_iter()
                .map(|(k, v)| if under(&k) { (moved(&k), v) } else { (k, v) })
                .collect();
            *colours = renamed;
        }
        let folder = old.parent().map(|p| self.order_key(p)).unwrap_or_default();
        let (old_name, new_name) = (name_of(old), name_of(new));
        let same_folder = old.parent().zip(new.parent()).is_some_and(|(a, b)| same(a, b));
        if let Some(Value::Object(order)) = self.data.get_mut("order")
            && let Some(Value::Array(names)) = order.get_mut(&folder)
            && let Some(at) = names.iter().position(|n| n.as_str() == Some(old_name.as_str()))
        {
            if same_folder {
                names[at] = json!(new_name);
            } else {
                names.remove(at);
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MoveError {
    #[error("A folder can not go into itself.")]
    IntoItself,
    #[error("{0}")]
    Io(#[from] io::Error),
}

fn copy_tree(src: &Path, dst: &Path) -> io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        copy_file(src, dst)
    }
}

/// Copies a file and keeps its modified time, as `shutil.copy2` does.
pub fn copy_file(src: &Path, dst: &Path) -> io::Result<()> {
    fs::copy(src, dst)?;
    if let Ok(modified) = fs::metadata(src).and_then(|m| m.modified()) {
        let _ = fs::File::options().write(true).open(dst).and_then(|f| f.set_modified(modified));
    }
    Ok(())
}

/// Moves a file or folder; across disks it copies and then removes the original.
fn move_path(src: &Path, dst: &Path) -> io::Result<()> {
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    copy_tree(src, dst)?;
    if src.is_dir() { fs::remove_dir_all(src) } else { fs::remove_file(src) }
}

/// Moves a clip or folder into `folder`. Gives back where it went; the same path when it is there already.
pub fn move_into(src: &Path, folder: &Path) -> Result<PathBuf, MoveError> {
    if src.parent().is_some_and(|p| same(p, folder)) {
        return Ok(src.to_owned());
    }
    if src.is_dir() && norm(folder).starts_with(&format!("{}{MAIN_SEPARATOR}", norm(src))) {
        return Err(MoveError::IntoItself);
    }
    let dst = unique_path(&folder.join(name_of(src)));
    move_path(src, &dst)?;
    Ok(dst)
}

/// What the clipboard held, ready to be saved.
#[derive(Debug, Clone, Default)]
pub struct Saving {
    pub files: Vec<PathBuf>,
    pub picture_png: Option<Vec<u8>>,
    pub text: Option<String>,
}

/// Saves what was on the clipboard into `folder`. Files win over a picture, a picture over text. Gives back what
/// was written; an error says what to tell the person.
pub fn save_clipboard(folder: &Path, saving: &Saving, stamp: &str) -> Result<Vec<PathBuf>, String> {
    let mut saved = Vec::new();
    if !saving.files.is_empty() {
        for src in saving.files.iter().filter(|s| s.exists()) {
            let name = src.file_name().map(|n| n.to_owned()).unwrap_or_default();
            let dst = unique_path(&folder.join(name));
            copy_tree(src, &dst).map_err(|e| e.to_string())?;
            saved.push(dst);
        }
        if saved.is_empty() {
            return Err("The files on the clipboard are not there any more.".to_owned());
        }
    } else if let Some(png) = &saving.picture_png {
        let dst = unique_path(&folder.join(format!("Clip {stamp}.png")));
        fs::write(&dst, png).map_err(|_| "The image on the clipboard could not be saved.".to_owned())?;
        saved.push(dst);
    } else if let Some(text) = saving.text.as_deref().filter(|t| !t.trim().is_empty()) {
        let dst = unique_path(&folder.join(text_clip_name(text, stamp)));
        fs::write(&dst, text).map_err(|e| e.to_string())?;
        saved.push(dst);
    } else {
        return Err(
            "The clipboard is empty (or holds something that is not text, an image or files).".to_owned()
        );
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../fixtures/sample_home/snippets/.luminescent.json");

    fn folder_with_sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temp folder");
        fs::write(dir.path().join(".luminescent.json"), SAMPLE).expect("the sample is written");
        dir
    }

    fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("the folder is made");
        }
        fs::write(&path, text).expect("the file is written");
        path
    }

    #[test]
    fn the_sample_meta_file_is_saved_back_unchanged() {
        let dir = folder_with_sample();
        let meta = Meta::load(dir.path());
        meta.save().expect("saved");
        assert_eq!(fs::read_to_string(dir.path().join(".luminescent.json")).expect("read"), SAMPLE);
    }

    #[test]
    fn the_sample_is_read_for_colours_favourites_and_order() {
        let dir = folder_with_sample();
        let meta = Meta::load(dir.path());
        assert_eq!(meta.folder_colour(&dir.path().join("Work")).as_deref(), Some("#3E63DD"));
        assert_eq!(meta.favourites(), vec![dir.path().join("hello.txt")]);
        assert_eq!(meta.order_of(dir.path()), vec!["Work", "hello.txt"]);
    }

    #[test]
    fn text_files_are_told_apart_by_what_they_hold() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let at = |name: &str, text: &str| classify(&write(dir.path(), name, text), 20);
        assert_eq!(
            at("Site.txt", "https://example.com/a\n"),
            Kind::Link { title: "Site".into(), url: "example.com/a".into() }
        );
        assert_eq!(at("blue.txt", "#3E63DD"), Kind::Colour("#3E63DD".into()));
        assert_eq!(at("short.txt", "#fff"), Kind::Colour("#fff".into()));
        assert_eq!(at("hash.txt", "#nothex"), Kind::Text("#nothex".into()));
        assert_eq!(at("two.txt", "https://a.b\nmore"), Kind::Text("https://a.b\nmore".into()));
        assert_eq!(at("code.py", "print(1)"), Kind::Code("print(1)".into()));
        assert_eq!(at("note.md", "  hello  "), Kind::Text("hello".into()));
        assert_eq!(at("empty.txt", ""), Kind::Text("empty.txt".into()));
        assert_eq!(at("long name that goes on and on.zip", "x"), Kind::File("long name that goes …".into()));
        assert_eq!(at("PHOTO.JPG", ""), Kind::Image);
        assert_eq!(at("clip.mkv", ""), Kind::Video);
    }

    #[test]
    fn a_text_card_keeps_the_first_300_characters_and_a_code_card_400() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let long = "a\n".repeat(500);
        let Kind::Text(text) = classify(&write(dir.path(), "t.txt", &long), 20) else { panic!("not text") };
        assert_eq!(text.chars().count(), 300);
        let Kind::Code(code) = classify(&write(dir.path(), "c.py", &long), 20) else { panic!("not code") };
        assert_eq!(code.chars().count(), 400);
    }

    #[test]
    fn ages_are_short() {
        let now: ZonedDateTime = "2026-11-27T12:00:00[UTC]".parse().expect("a time");
        let ago = |secs: i64| {
            let then = now.checked_sub(jiff::Span::new().seconds(secs)).expect("earlier");
            age_text(&then, &now)
        };
        assert_eq!(ago(5), "now");
        assert_eq!(ago(120), "2m");
        assert_eq!(ago(3 * 3600), "3h");
        assert_eq!(ago(30 * 3600), "yesterday");
        assert_eq!(ago(4 * 86_400), "4d");
        assert_eq!(ago(20 * 86_400), "7 Nov");
        let ahead = now.checked_add(jiff::Span::new().hours(2)).expect("later");
        assert_eq!(age_text(&ahead, &now), "now", "a clock that ran ahead never gives a negative age");
    }

    #[test]
    fn sizes_are_short() {
        assert_eq!(size_text(0), "0 B");
        assert_eq!(size_text(1023), "1023 B");
        assert_eq!(size_text(2048), "2 KB");
        assert_eq!(size_text(1_572_864), "1.5 MB");
        assert_eq!(size_text(5 * 1024 * 1024 * 1024), "5.0 GB");
    }

    #[test]
    fn placeholders_fill_in_and_other_braces_stay() {
        let now: DateTime = "2026-11-15T14:05:00".parse().expect("a time");
        let text = "{date} {time} {datetime} {weekday} {clipboard} {other} {{date}}";
        assert_eq!(
            fill_placeholders(text, "clip", now),
            "2026-11-15 14:05 2026-11-15 14:05 Sunday clip {other} {2026-11-15}"
        );
        assert_eq!(fill_placeholders("x = {1}", "", now), "x = {1}");
    }

    #[test]
    fn a_name_that_is_taken_gets_a_number() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let first = write(dir.path(), "a.txt", "1");
        assert_eq!(unique_path(&first), dir.path().join("a (2).txt"));
        write(dir.path(), "a (2).txt", "2");
        assert_eq!(unique_path(&first), dir.path().join("a (3).txt"));
        assert_eq!(unique_path(&dir.path().join("new.txt")), dir.path().join("new.txt"));
    }

    #[test]
    fn a_text_clip_is_named_after_its_first_line() {
        assert_eq!(text_clip_name("  Hello, world!\nsecond", "S"), "Hello world.txt");
        assert_eq!(text_clip_name("!!!", "2026-11-27 10-00-00"), "Clip 2026-11-27 10-00-00.txt");
        let long = "x".repeat(60);
        assert_eq!(text_clip_name(&long, "S").len(), 44);
    }

    #[test]
    fn folders_come_first_by_name_and_clips_in_the_kept_order() {
        let dir = folder_with_sample();
        write(dir.path(), "hello.txt", "Hello");
        write(dir.path(), "b.txt", "b");
        write(dir.path(), "A.txt", "a");
        fs::create_dir(dir.path().join("Work")).expect("a folder");
        fs::create_dir(dir.path().join("archive")).expect("a folder");
        write(dir.path(), "Work/one.txt", "1");
        write(dir.path(), ".hidden", "x");
        let meta = Meta::load(dir.path());
        let listing = list_folder(&meta, dir.path(), 20);
        let names: Vec<_> = listing.folders.iter().map(|f| name_of(&f.path)).collect();
        assert_eq!(names, ["archive", "Work"]);
        assert_eq!(listing.folders[1].count, 1);
        let clips: Vec<_> = listing.clips.iter().map(|c| name_of(&c.path)).collect();
        assert_eq!(clips, ["hello.txt", "A.txt", "b.txt"], "the order list first, the rest by name");
        assert_eq!(listing.favourites.len(), 1);
        assert!(listing.message.is_empty());
        let inside = list_folder(&meta, &dir.path().join("Work"), 20);
        assert!(inside.favourites.is_empty(), "favourites show at the top level only");
    }

    #[test]
    fn an_empty_folder_and_a_missing_one_say_so() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let meta = Meta::load(dir.path());
        assert!(list_folder(&meta, dir.path(), 20).message.starts_with("This folder is empty."));
        let missing = list_folder(&meta, &dir.path().join("nope"), 20);
        assert!(missing.message.starts_with("Cannot open folder:"));
    }

    #[test]
    fn search_looks_at_names_and_text_in_every_folder() {
        let dir = tempfile::tempdir().expect("a temp folder");
        write(dir.path(), "invoice.txt", "pay me");
        write(dir.path(), "Work/notes.md", "the INVOICE is late");
        write(dir.path(), "Work/pic.png", "invoice but not text");
        write(dir.path(), "Invoices/readme.txt", "x");
        let meta = Meta::load(dir.path());
        let found = search(&meta, "  Invoice ", 20);
        let clips: Vec<_> = found.clips.iter().map(|c| relative(&c.path, dir.path())).collect();
        let sep = MAIN_SEPARATOR;
        assert_eq!(clips, ["invoice.txt".to_owned(), format!("Work{sep}notes.md")]);
        assert_eq!(found.folders.len(), 1, "the Invoices folder matches by name");
        assert_eq!(found.clips[1].where_, "Work");
        assert_eq!(found.clips[0].where_, "");
        let none = search(&meta, "zzz", 20);
        assert_eq!(none.message, "Nothing in any folder matches \"zzz\".");
    }

    #[test]
    fn a_search_stops_at_the_limit_and_says_so() {
        let dir = tempfile::tempdir().expect("a temp folder");
        for i in 0..(SEARCH_LIMIT + 20) {
            write(dir.path(), &format!("clip {i:03}.txt"), "x");
        }
        let meta = Meta::load(dir.path());
        let found = search(&meta, "clip", 20);
        assert_eq!(found.clips.len(), SEARCH_LIMIT);
        assert_eq!(found.message, "Showing the first 200 matches.");
    }

    #[test]
    fn favourites_can_be_put_before_one_another_and_taken_out() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let mut meta = Meta::load(dir.path());
        let (a, b, c) = (dir.path().join("a.txt"), dir.path().join("b.txt"), dir.path().join("c.txt"));
        meta.set_favourite(&a, true, None);
        meta.set_favourite(&b, true, None);
        meta.set_favourite(&c, true, Some(&a));
        assert_eq!(meta.favourites(), vec![c.clone(), a.clone(), b.clone()]);
        meta.set_favourite(&b, true, Some(&c));
        assert_eq!(meta.favourites(), vec![b.clone(), c.clone(), a.clone()]);
        meta.set_favourite(&c, false, None);
        assert_eq!(meta.favourites(), vec![b, a]);
        assert!(!meta.is_favourite(&c));
    }

    #[test]
    fn placing_a_clip_before_another_writes_the_folder_order() {
        let dir = tempfile::tempdir().expect("a temp folder");
        for n in ["a.txt", "b.txt", "c.txt"] {
            write(dir.path(), n, n);
        }
        let mut meta = Meta::load(dir.path());
        meta.place_before(&dir.path().join("c.txt"), &dir.path().join("a.txt"));
        assert_eq!(meta.order_of(dir.path()), ["c.txt", "a.txt", "b.txt"]);
        meta.place_before(&dir.path().join("a.txt"), &dir.path().join("a.txt"));
        assert_eq!(meta.order_of(dir.path()), ["c.txt", "a.txt", "b.txt"]);
        meta.save().expect("saved");
        let again = Meta::load(dir.path());
        assert_eq!(again.order_of(dir.path()), ["c.txt", "a.txt", "b.txt"]);
        let text = fs::read_to_string(dir.path().join(".luminescent.json")).expect("read");
        assert!(text.contains("\"order\": {\n    \".\": ["), "the top level is written as a dot: {text}");
    }

    #[test]
    fn the_order_of_a_file_written_with_an_empty_name_is_kept_in_that_key() {
        let dir = folder_with_sample();
        for n in ["hello.txt", "x.txt"] {
            write(dir.path(), n, n);
        }
        let mut meta = Meta::load(dir.path());
        meta.place_before(&dir.path().join("x.txt"), &dir.path().join("hello.txt"));
        let text = serde_json::to_string(&meta.data).expect("json");
        assert!(text.contains(r#""order":{"":["x.txt","hello.txt"]}"#), "{text}");
    }

    #[test]
    fn a_move_or_rename_takes_favourite_colour_and_place_along() {
        let dir = folder_with_sample();
        let mut meta = Meta::load(dir.path());
        meta.set_favourite(&dir.path().join("Work").join("deep.txt"), true, None);
        meta.path_moved(&dir.path().join("Work"), &dir.path().join("Jobs"));
        assert_eq!(meta.folder_colour(&dir.path().join("Jobs")).as_deref(), Some("#3E63DD"));
        assert_eq!(meta.folder_colour(&dir.path().join("Work")), None);
        assert!(meta.is_favourite(&dir.path().join("Jobs").join("deep.txt")));
        meta.path_moved(&dir.path().join("hello.txt"), &dir.path().join("hi.txt"));
        assert!(meta.is_favourite(&dir.path().join("hi.txt")));
        assert_eq!(meta.order_of(dir.path()), ["Jobs", "hi.txt"]);
    }

    #[test]
    fn moving_a_clip_into_a_folder_leaves_the_order() {
        let dir = tempfile::tempdir().expect("a temp folder");
        write(dir.path(), "a.txt", "a");
        write(dir.path(), "b.txt", "b");
        fs::create_dir(dir.path().join("F")).expect("a folder");
        let mut meta = Meta::load(dir.path());
        meta.place_before(&dir.path().join("b.txt"), &dir.path().join("a.txt"));
        meta.path_moved(&dir.path().join("b.txt"), &dir.path().join("F").join("b.txt"));
        assert_eq!(meta.order_of(dir.path()), ["a.txt"]);
    }

    #[test]
    fn moves_pick_a_free_name_and_refuse_to_go_into_themselves() {
        let dir = tempfile::tempdir().expect("a temp folder");
        write(dir.path(), "a.txt", "root");
        write(dir.path(), "F/a.txt", "inside");
        write(dir.path(), "F/Sub/x.txt", "x");
        let moved = move_into(&dir.path().join("a.txt"), &dir.path().join("F")).expect("moved");
        assert_eq!(moved, dir.path().join("F").join("a (2).txt"));
        assert_eq!(fs::read_to_string(&moved).expect("read"), "root");
        assert!(!dir.path().join("a.txt").exists());
        let same_place = move_into(&dir.path().join("F").join("a.txt"), &dir.path().join("F")).expect("fine");
        assert_eq!(same_place, dir.path().join("F").join("a.txt"));
        let into_self = move_into(&dir.path().join("F"), &dir.path().join("F").join("Sub"));
        assert!(matches!(into_self, Err(MoveError::IntoItself)));
        let up = move_into(&dir.path().join("F").join("Sub"), dir.path()).expect("moved up");
        assert_eq!(up, dir.path().join("Sub"));
        assert!(dir.path().join("Sub").join("x.txt").exists());
    }

    #[test]
    fn the_clipboard_is_saved_as_files_by_kind() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let src = write(dir.path(), "from/original.txt", "kept");
        let out = dir.path().join("out");
        fs::create_dir(&out).expect("a folder");
        let saved = save_clipboard(&out, &Saving { files: vec![src.clone()], ..Saving::default() }, "S")
            .expect("saved");
        assert_eq!(saved, vec![out.join("original.txt")]);
        let again =
            save_clipboard(&out, &Saving { files: vec![src], ..Saving::default() }, "S").expect("saved");
        assert_eq!(again, vec![out.join("original (2).txt")]);
        let picture =
            Saving { picture_png: Some(vec![1, 2, 3]), text: Some("ignored".into()), ..Saving::default() };
        let saved = save_clipboard(&out, &picture, "2026-11-27 10-00-00").expect("saved");
        assert_eq!(saved, vec![out.join("Clip 2026-11-27 10-00-00.png")]);
        let text = Saving { text: Some("Dear sir\nbody".into()), ..Saving::default() };
        assert_eq!(save_clipboard(&out, &text, "S").expect("saved"), vec![out.join("Dear sir.txt")]);
        let blank = Saving { text: Some("  \n ".into()), ..Saving::default() };
        assert!(save_clipboard(&out, &blank, "S").unwrap_err().starts_with("The clipboard is empty"));
        let gone = Saving { files: vec![out.join("missing.bin")], ..Saving::default() };
        assert!(save_clipboard(&out, &gone, "S").is_err());
    }

    #[test]
    fn a_copied_file_keeps_its_modified_time() {
        let dir = tempfile::tempdir().expect("a temp folder");
        let src = write(dir.path(), "a.txt", "a");
        let old = SystemTime::now() - std::time::Duration::from_secs(86_400 * 3);
        fs::File::options().write(true).open(&src).expect("open").set_modified(old).expect("set");
        let dst = dir.path().join("b.txt");
        copy_file(&src, &dst).expect("copied");
        let got = fs::metadata(&dst).and_then(|m| m.modified()).expect("modified");
        let diff = got.duration_since(old).unwrap_or_else(|e| e.duration());
        assert!(diff.as_secs() < 2);
    }

    #[test]
    fn normpath_cleans_dots_without_the_disk() {
        assert_eq!(normpath(Path::new("/a/./b/../c")), PathBuf::from("/a/c"));
        assert_eq!(normpath(Path::new("")), PathBuf::from("."));
        assert_eq!(relative(Path::new("/a/b/c"), Path::new("/a")), format!("b{MAIN_SEPARATOR}c"));
        assert_eq!(relative(Path::new("/a"), Path::new("/a/")), ".");
    }
}
