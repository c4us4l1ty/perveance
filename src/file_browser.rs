//! Bounded directory listing for the GUI path picker, not a security boundary.
use std::path::{Component, Path, PathBuf};

pub const MAX_DIRECTORY_ENTRIES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind { Directory, File, Symlink }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrowserEntry { pub name: String, pub kind: EntryKind }

#[derive(Debug, PartialEq, Eq)]
pub struct DirectoryListing { pub entries: Vec<BrowserEntry>, pub skipped: usize }

/// Reads at most 513 entries. Counts hidden and unsupported names toward the
/// cap so filters cannot hide an unbounded scan. Symlinks are listed but never
/// followed here. Input directories must be trusted and may themselves be links.
pub fn list_directory(path: &Path, show_hidden: bool) -> Result<DirectoryListing, String> {
    let iter = std::fs::read_dir(path).map_err(|e| format!("Cannot read directory: {e}"))?;
    let mut entries = Vec::new();
    let mut skipped = 0;
    for (i, item) in iter.enumerate() {
        if i >= MAX_DIRECTORY_ENTRIES {
            return Err(format!("Directory exceeds {MAX_DIRECTORY_ENTRIES} entries; type the path manually."));
        }
        let item = item.map_err(|e| format!("Cannot read directory entry: {e}"))?;
        let name = match item.file_name().into_string() {
            Ok(name) => name,
            Err(_) => { skipped += 1; continue; }
        };
        if !show_hidden && name.starts_with('.') { continue; }
        let ty = item.file_type().map_err(|e| format!("Cannot inspect directory entry: {e}"))?;
        let kind = if ty.is_dir() { EntryKind::Directory }
            else if ty.is_file() { EntryKind::File }
            else if ty.is_symlink() { EntryKind::Symlink }
            else { skipped += 1; continue; };
        entries.push(BrowserEntry { name, kind });
    }
    entries.sort_by(|a, b| {
        let order = |kind: &EntryKind| if *kind == EntryKind::Directory { 0 } else { 1 };
        order(&a.kind).cmp(&order(&b.kind)).then_with(|| a.name.cmp(&b.name))
    });
    Ok(DirectoryListing { entries, skipped })
}

/// File names are a single nonempty component, not a path or parent traversal.
/// This checks syntax only; it does not reserve a destination or follow links.
pub fn chosen_file(directory: &Path, filename: &str) -> Result<PathBuf, String> {
    let mut components = Path::new(filename).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None) if name == filename => Ok(directory.join(name)),
        _ => Err("Enter one file name, without directories or ..".into()),
    }
}
