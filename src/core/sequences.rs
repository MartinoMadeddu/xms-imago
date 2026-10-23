//! File sequences and UDIM tiles.
//!
//! - **UDIM:** a texture path with `<UDIM>` (or `<udim>`) in it stands for
//!   one file per UV tile, the tag replaced by the tile's number: 1001 for
//!   the unit square, 1002 to its right, 1011 above it (`core::uv::udim`).
//!   `udim_tiles` finds the files a path stands for.
//! - **Sequences:** files whose names differ only by the last run of digits
//!   in them (`shot.0001.exr`, `shot.0002.exr`) are one sequence;
//!   `collapse` lists a folder's files with each sequence as one entry, and,
//!   asked to, a sequence numbered as UDIM tiles as one `<UDIM>` path.

use std::path::{Path, PathBuf};

/// Where the UDIM tag is in a path, and how long it is: `<UDIM>` or `<udim>`.
pub fn udim_tag(path: &str) -> Option<(usize, usize)> {
    let lower = path.to_ascii_lowercase();
    lower.find("<udim>").map(|at| (at, "<udim>".len()))
}

/// Whether a path stands for UDIM tiles.
pub fn is_udim(path: &Path) -> bool { udim_tag(&path.to_string_lossy()).is_some() }

/// The tile files a `<UDIM>` path stands for, by tile number, in order:
/// the files in its folder whose names are the path's with a number from
/// 1001 to 1999 in place of the tag. Empty for a path without the tag.
pub fn udim_tiles(path: &Path) -> Vec<(u32, PathBuf)> {
    let text = path.to_string_lossy().to_string();
    let Some((at, len)) = udim_tag(&text) else { return vec![] };
    let (before, after) = (&text[..at], &text[at + len..]);
    // The tag in the file name, not in a folder: the folder is the one before it.
    let split = before.rfind(['/', '\\']).map_or(0, |k| k + 1);
    let (dir, prefix) = (&before[..split], &before[split..]);
    if after.contains(['/', '\\']) { return vec![]; }
    let dir = if dir.is_empty() { Path::new(".") } else { Path::new(dir) };
    let Ok(read) = std::fs::read_dir(dir) else { return vec![] };
    let mut tiles: Vec<(u32, PathBuf)> = read.filter_map(|e| e.ok()).filter_map(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        let number = name.strip_prefix(prefix)?.strip_suffix(after)?;
        if number.len() != 4 || !number.bytes().all(|b| b.is_ascii_digit()) { return None; }
        let n: u32 = number.parse().ok()?;
        (1001..=1999).contains(&n).then(|| (n, e.path()))
    }).collect();
    tiles.sort_by_key(|(n, _)| *n);
    tiles
}

/// The column and row of a UDIM tile: 1001 is (0, 0), 1002 (1, 0), 1011 (0, 1).
pub fn tile_place(n: u32) -> (u32, u32) {
    let k = n.saturating_sub(1001);
    (k % 10, k / 10)
}

/// Whether a texture path names something there: the file, or for a
/// `<UDIM>` path at least one of its tiles.
pub fn texture_exists(path: &str) -> bool {
    // `$JOB/tex/wood.png`: its variables put in.
    let path = crate::core::vars::expand(path.trim(), &[]);
    let p = Path::new(&path);
    if is_udim(p) { !udim_tiles(p).is_empty() } else { p.exists() }
}

/// One entry of a folder's listing, sequences collapsed.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    /// A file on its own.
    File(PathBuf),
    /// Files numbered alike: the name shown (`shot.####.exr`), the numbers,
    /// first to last, and the files in that order.
    Sequence { name: String, numbers: Vec<u64>, files: Vec<PathBuf> },
    /// A sequence numbered as UDIM tiles: the `<UDIM>` path, and its tiles.
    Udim { path: PathBuf, tiles: Vec<u32> },
}

impl Entry {
    /// What the listing shows for it.
    pub fn label(&self) -> String {
        match self {
            Entry::File(p) => p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            Entry::Sequence { name, numbers, .. } => {
                let (a, b) = (numbers.first().copied().unwrap_or(0), numbers.last().copied().unwrap_or(0));
                format!("{name}   {a}-{b} ({} files)", numbers.len())
            }
            Entry::Udim { path, tiles } => {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let (a, b) = (tiles.first().copied().unwrap_or(0), tiles.last().copied().unwrap_or(0));
                format!("{name}   {a}-{b} ({} tiles)", tiles.len())
            }
        }
    }

    /// The path picking it gives: the file, a sequence's first file, the
    /// `<UDIM>` path.
    pub fn path(&self) -> PathBuf {
        match self {
            Entry::File(p) => p.clone(),
            Entry::Sequence { files, .. } => files.first().cloned().unwrap_or_default(),
            Entry::Udim { path, .. } => path.clone(),
        }
    }
}

/// A file name cut around its last run of digits: what is before, the
/// digits, what is after. `None` when it has no digits.
fn numbered(name: &str) -> Option<(&str, &str, &str)> {
    let end = name.rfind(|c: char| c.is_ascii_digit())? + 1;
    let start = name[..end].rfind(|c: char| !c.is_ascii_digit()).map_or(0, |k| k + 1);
    Some((&name[..start], &name[start..end], &name[end..]))
}

/// A folder's files, in the order given, with the files that make a
/// sequence (two or more names alike but for their last number) as one
/// entry where the first of them was. With `udim`, a sequence numbered
/// 1001 to 1999 in four digits is one `<UDIM>` path.
pub fn collapse(files: &[PathBuf], udim: bool) -> Vec<Entry> {
    use std::collections::HashMap;
    // Files by their name around the number: (folder, before, digits' length, after).
    let key = |p: &PathBuf| -> Option<(PathBuf, String, usize, String, u64)> {
        let name = p.file_name()?.to_string_lossy().to_string();
        let (a, n, b) = numbered(&name)?;
        Some((p.parent().map(Path::to_path_buf).unwrap_or_default(), a.to_string(), n.len(), b.to_string(), n.parse().ok()?))
    };
    let mut groups: HashMap<(PathBuf, String, usize, String), Vec<(u64, PathBuf)>> = HashMap::new();
    for f in files {
        if let Some((d, a, w, b, n)) = key(f) { groups.entry((d, a, w, b)).or_default().push((n, f.clone())); }
    }
    let mut out = vec![];
    let mut done: std::collections::HashSet<(PathBuf, String, usize, String)> = Default::default();
    for f in files {
        let Some((d, a, w, b, _)) = key(f) else { out.push(Entry::File(f.clone())); continue };
        let k = (d.clone(), a.clone(), w, b.clone());
        let members = &groups[&k];
        if members.len() < 2 { out.push(Entry::File(f.clone())); continue; }
        if !done.insert(k) { continue; }
        let mut members = members.clone();
        members.sort_by_key(|(n, _)| *n);
        let numbers: Vec<u64> = members.iter().map(|(n, _)| *n).collect();
        if udim && w == 4 && numbers.iter().all(|n| (1001..=1999).contains(n)) {
            out.push(Entry::Udim { path: d.join(format!("{a}<UDIM>{b}")), tiles: numbers.iter().map(|n| *n as u32).collect() });
        } else {
            out.push(Entry::Sequence {
                name: format!("{a}{}{b}", "#".repeat(w)),
                numbers,
                files: members.into_iter().map(|(_, p)| p).collect(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequences_and_udims_collapse() {
        let p = |s: &str| PathBuf::from(format!("/tex/{s}"));
        let files: Vec<PathBuf> = ["body.1001.exr", "body.1002.exr", "body.1011.exr", "readme.txt", "shot.0001.png", "shot.0002.png", "shot.0003.png", "v2_final.png"]
            .iter().map(|s| p(s)).collect();
        let off = collapse(&files, false);
        assert_eq!(off.len(), 4, "{off:?}");
        assert!(matches!(&off[0], Entry::Sequence { name, numbers, .. } if name == "body.####.exr" && numbers == &vec![1001, 1002, 1011]));
        assert_eq!(off[0].path(), p("body.1001.exr"), "a sequence picks its first file");
        assert_eq!(off[1], Entry::File(p("readme.txt")));
        assert!(matches!(&off[2], Entry::Sequence { name, .. } if name == "shot.####.png"));
        assert_eq!(off[3], Entry::File(p("v2_final.png")), "a number on its own is no sequence");
        let on = collapse(&files, true);
        assert_eq!(on[0], Entry::Udim { path: p("body.<UDIM>.exr"), tiles: vec![1001, 1002, 1011] });
        assert_eq!(on[0].label(), "body.<UDIM>.exr   1001-1011 (3 tiles)");
        assert!(matches!(&on[2], Entry::Sequence { .. }), "frames 1 to 3 are not tiles");
    }

    #[test]
    fn a_udim_path_finds_its_tiles() {
        let dir = std::env::temp_dir().join("xms_udim_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["1001", "1002", "1012", "0001", "1001x"] { std::fs::write(dir.join(format!("skin.{n}.png")), b"x").unwrap(); }
        for tag in ["<UDIM>", "<udim>"] {
            let tiles = udim_tiles(&dir.join(format!("skin.{tag}.png")));
            assert_eq!(tiles.iter().map(|(n, _)| *n).collect::<Vec<_>>(), vec![1001, 1002, 1012], "{tag}");
        }
        assert!(texture_exists(&dir.join("skin.<UDIM>.png").to_string_lossy()));
        assert!(!texture_exists(&dir.join("bone.<UDIM>.png").to_string_lossy()));
        assert_eq!((tile_place(1001), tile_place(1002), tile_place(1012)), ((0, 0), (1, 0), (1, 1)));
        assert!(udim_tiles(&dir.join("skin.1001.png")).is_empty(), "no tag, no tiles");
    }
}
