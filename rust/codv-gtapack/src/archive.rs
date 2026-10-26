//! GTA V's archives read in place: an archive is memory-mapped and only its
//! tables and the entries asked for are read. Paths and bytes are the ones
//! `rage extract --recursive` writes (an entry's path within its archive,
//! nested archives as folders).
//!
//! From SK8V rust/gta-archives/src/lib.rs (Apache-2.0, NOTICE), which follows
//! rage-cli's `rpf.rs` (VIRUXE, Unlicense); only what CODV's setup reads is kept.

use anyhow::Result;
use rpf_archive::{DirNode, RpfArchive, build_directory_tree, list_all_files};
pub use rpf_archive::{FileRef, GtaKeys};
use std::path::Path;
use std::sync::Arc;

enum Buffer {
    Owned(Vec<u8>),
    Mapped(memmap2::Mmap),
}

/// The bytes an archive reads from: a shared buffer and this archive's window of it.
#[derive(Clone)]
struct Backing {
    buffer: Arc<Buffer>,
    range: std::ops::Range<usize>,
}

impl Backing {
    fn bytes(&self) -> &[u8] {
        let all: &[u8] = match &*self.buffer {
            Buffer::Owned(v) => v,
            Buffer::Mapped(m) => m,
        };
        &all[self.range.clone()]
    }
}

pub struct Archive {
    archive: RpfArchive,
    root: DirNode,
    data: Backing,
}

impl Archive {
    /// Memory-maps `path` (a plain read where mapping fails, e.g. an empty file).
    pub fn open(path: &Path, keys: &GtaKeys) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        // SAFETY: read-only map; an archive changed by another process meanwhile reads as garbage, not UB.
        let data = match unsafe { memmap2::Mmap::map(&file) } {
            Ok(map) => Backing { range: 0..map.len(), buffer: Arc::new(Buffer::Mapped(map)) },
            Err(_) => {
                let v = std::fs::read(path)?;
                Backing { range: 0..v.len(), buffer: Arc::new(Buffer::Owned(v)) }
            }
        };
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        Self::from_backing(data, name, keys)
    }

    fn from_backing(data: Backing, name: &str, keys: &GtaKeys) -> Result<Self> {
        let archive = RpfArchive::parse(data.bytes(), name, Some(keys))?;
        let root = build_directory_tree(&archive.entries);
        Ok(Self { archive, root, data })
    }

    /// A nested `.rpf`: a window onto this archive's bytes when stored plain (always, in practice).
    pub fn open_nested(&self, file: &FileRef, keys: &GtaKeys) -> Result<Self> {
        if let Some(range) = self.stored_range(file) {
            let mut data = self.data.clone();
            data.range = data.range.start + range.start..data.range.start + range.end;
            return Self::from_backing(data, &file.name, keys);
        }
        let bytes = self.extract(file, keys)?;
        let data = Backing { range: 0..bytes.len(), buffer: Arc::new(Buffer::Owned(bytes)) };
        Self::from_backing(data, &file.name, keys)
    }

    fn stored_range(&self, file: &FileRef) -> Option<std::ops::Range<usize>> {
        let rpf_archive::RpfEntryKind::BinaryFile { file_offset, file_size, uncompressed_size, is_encrypted } =
            self.archive.entries[file.entry_index].kind
        else {
            return None;
        };
        if is_encrypted || (file_size != 0 && file_size != uncompressed_size) || uncompressed_size == 0 {
            return None;
        }
        let offset = match self.archive.version {
            rpf_archive::RpfVersion::V7 => file_offset as usize * 512,
            _ => file_offset as usize,
        };
        let start = self.archive.start_offset + offset;
        let end = start.checked_add(uncompressed_size as usize)?;
        (end <= self.data.range.len()).then_some(start..end)
    }

    pub fn files(&self) -> Vec<FileRef> {
        list_all_files(&self.root).into_iter().cloned().collect()
    }

    /// The entry's bytes as `rage extract` writes them.
    pub fn extract(&self, file: &FileRef, keys: &GtaKeys) -> Result<Vec<u8>> {
        self.archive.extract_entry(self.data.bytes(), &self.archive.entries[file.entry_index], keys.into())
    }
}

/// One entry found by [`walk`]: its archive (kept open for reading later) and its path.
#[derive(Clone)]
pub struct Entry {
    pub archive: Arc<Archive>,
    pub file: FileRef,
    /// The path `rage extract --recursive` writes it at, below the output folder.
    pub path: String,
}

impl Entry {
    pub fn read(&self, keys: &GtaKeys) -> Result<Vec<u8>> {
        self.archive.extract(&self.file, keys)
    }
}

/// Every file of `archive` and its nested archives whose path `want` accepts,
/// in archive order. Nested archives that fail to open are reported to `failed`.
pub fn walk(
    archive: Arc<Archive>,
    keys: &GtaKeys,
    want: &dyn Fn(&str) -> bool,
    failed: &mut Vec<String>,
) -> Vec<Entry> {
    let mut out = Vec::new();
    walk_inner(archive, "", keys, want, failed, &mut out, 0);
    out
}

fn walk_inner(
    archive: Arc<Archive>,
    prefix: &str,
    keys: &GtaKeys,
    want: &dyn Fn(&str) -> bool,
    failed: &mut Vec<String>,
    out: &mut Vec<Entry>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    for file in archive.files() {
        let path = if prefix.is_empty() { file.path.clone() } else { format!("{prefix}/{}", file.path) };
        if file.name.to_lowercase().ends_with(".rpf") {
            match archive.open_nested(&file, keys) {
                Ok(nested) => walk_inner(Arc::new(nested), &path, keys, want, failed, out, depth + 1),
                Err(e) => failed.push(format!("failed to open nested {path}: {e}")),
            }
        } else if want(&path) {
            out.push(Entry { archive: archive.clone(), file, path });
        }
    }
}

/// rage-cli's match: `*` matches any run (including `/`); a pattern without `*` is a substring.
pub fn matches_pattern(path: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return path.contains(pattern);
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !path.starts_with(first) {
        return false;
    }
    let mut rest = &path[first.len()..];
    for part in &parts[1..parts.len() - 1] {
        if part.is_empty() {
            continue;
        }
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

pub fn any_match(path: &str, patterns: &[String]) -> bool {
    patterns.is_empty() || patterns.iter().any(|p| matches_pattern(path, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_like_rage() {
        assert!(matches_pattern("common/data/ai/weapons.meta", "*common/data/ai/weapons.meta"));
        assert!(matches_pattern("x64/data/cdimages/scaleform_generic.rpf/hud.gfx", "*scaleform_generic.rpf/hud.gfx"));
        assert!(!matches_pattern("common/data/ai/weaponcomponents.meta", "*common/data/ai/weapons.meta"));
        assert!(matches_pattern("a/content.xml", "content.xml"));
    }
}
