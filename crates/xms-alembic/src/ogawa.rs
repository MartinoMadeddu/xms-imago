//! Ogawa, the container Alembic files are written in: a tree of groups and
//! data blocks addressed by their offset in the file.
//!
//! - The file starts with `Ogawa`, a byte that is `0xff` once the file was
//!   closed cleanly (frozen), the version (two bytes, 1) and the offset of the
//!   top group.
//! - A group is a count of children, then one 8-byte child offset each. A
//!   child with the top bit set is a data block, else a group; 0 is an empty
//!   group and the top bit alone an empty data block.
//! - A data block is its size (8 bytes), then its bytes.
//!
//! Everything is little-endian.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::Error;

/// A child offset with this bit set is a data block.
const DATA_BIT: u64 = 0x8000_0000_0000_0000;

/// Where an archive's bytes are read from: all in memory, or a file read as
/// asked.
pub(crate) enum Source {
    Bytes(Arc<[u8]>),
    File { file: Mutex<File>, size: u64 },
}

impl Source {
    pub(crate) fn open(path: &Path) -> Result<Source, Error> {
        let file = File::open(path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
        let size = file.metadata().map_err(|e| Error::Io(e.to_string()))?.len();
        Ok(Source::File {
            file: Mutex::new(file),
            size,
        })
    }

    pub(crate) fn size(&self) -> u64 {
        match self {
            Source::Bytes(b) => b.len() as u64,
            Source::File { size, .. } => *size,
        }
    }

    /// `len` bytes at `pos`, all of them or an error.
    pub(crate) fn read(&self, pos: u64, len: u64) -> Result<Vec<u8>, Error> {
        let end = pos
            .checked_add(len)
            .ok_or(Error::Corrupt("a read past the end of the file"))?;
        if end > self.size() {
            return Err(Error::Corrupt("a read past the end of the file"));
        }
        match self {
            Source::Bytes(b) => Ok(b[pos as usize..end as usize].to_vec()),
            Source::File { file, .. } => {
                let mut f = file.lock().map_err(|_| Error::Io("the file is unavailable".into()))?;
                let mut out = vec![0; len as usize];
                f.seek(SeekFrom::Start(pos)).map_err(|e| Error::Io(e.to_string()))?;
                f.read_exact(&mut out).map_err(|e| Error::Io(e.to_string()))?;
                Ok(out)
            }
        }
    }

    pub(crate) fn u64_at(&self, pos: u64) -> Result<u64, Error> {
        let b = self.read(pos, 8)?;
        Ok(u64::from_le_bytes(b.try_into().expect("8 bytes")))
    }
}

/// The header of an Ogawa file: whether it was closed cleanly, and where its
/// top group is.
pub(crate) fn top_group(src: &Source) -> Result<u64, Error> {
    if src.size() < 16 {
        return Err(Error::NotAlembic("too short to be an Ogawa file".into()));
    }
    let h = src.read(0, 16)?;
    if &h[0..5] != b"Ogawa" {
        return Err(Error::NotAlembic(
            "not an Ogawa file (an Alembic file of the older HDF5 kind is not read)".into(),
        ));
    }
    if h[5] != 0xff {
        return Err(Error::NotAlembic(
            "the file was not closed cleanly while it was written".into(),
        ));
    }
    let version = u16::from(h[6]) << 8 | u16::from(h[7]);
    if version != 1 {
        return Err(Error::NotAlembic(format!("Ogawa version {version} is not known")));
    }
    Ok(u64::from_le_bytes(h[8..16].try_into().expect("8 bytes")))
}

/// A group: its children's offsets.
#[derive(Clone, Debug, Default)]
pub(crate) struct Group {
    children: Vec<u64>,
}

impl Group {
    /// The group at `pos` (0: an empty group).
    pub(crate) fn read(src: &Source, pos: u64) -> Result<Group, Error> {
        if pos == 0 {
            return Ok(Group::default());
        }
        let n = src.u64_at(pos)?;
        if n == 0 || n > src.size() / 8 {
            return Ok(Group::default());
        }
        let bytes = src.read(pos + 8, n * 8)?;
        let children = bytes
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().expect("8 bytes")))
            .collect();
        Ok(Group { children })
    }

    pub(crate) fn len(&self) -> usize {
        self.children.len()
    }

    pub(crate) fn is_group(&self, i: usize) -> bool {
        self.children.get(i).is_some_and(|c| c & DATA_BIT == 0)
    }

    pub(crate) fn is_data(&self, i: usize) -> bool {
        self.children.get(i).is_some_and(|c| c & DATA_BIT != 0)
    }

    /// Child `i`, a group (an empty one when it is not there).
    pub(crate) fn group(&self, src: &Source, i: usize) -> Result<Group, Error> {
        match self.children.get(i) {
            Some(&c) if c & DATA_BIT == 0 => Group::read(src, c),
            Some(_) => Err(Error::Corrupt("a data block where a group was expected")),
            None => Ok(Group::default()),
        }
    }

    /// Child `i`, a data block (an empty one when it is not there).
    pub(crate) fn data(&self, src: &Source, i: usize) -> Result<Data, Error> {
        match self.children.get(i) {
            Some(&c) if c & DATA_BIT != 0 => Data::read(src, c & !DATA_BIT),
            Some(_) => Err(Error::Corrupt("a group where a data block was expected")),
            None => Ok(Data::default()),
        }
    }
}

/// A data block: where its bytes start, and how many there are.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Data {
    pos: u64,
    pub(crate) size: u64,
}

impl Data {
    fn read(src: &Source, pos: u64) -> Result<Data, Error> {
        if pos == 0 {
            return Ok(Data::default());
        }
        let size = src.u64_at(pos)?;
        if size > src.size() {
            return Err(Error::Corrupt("a data block larger than the file"));
        }
        Ok(Data { pos: pos + 8, size })
    }

    /// All its bytes.
    pub(crate) fn bytes(&self, src: &Source) -> Result<Vec<u8>, Error> {
        self.slice(src, 0, self.size)
    }

    /// `len` of its bytes from `offset`.
    pub(crate) fn slice(&self, src: &Source, offset: u64, len: u64) -> Result<Vec<u8>, Error> {
        if len == 0 {
            return Ok(vec![]);
        }
        if offset + len > self.size {
            return Err(Error::Corrupt("a read past the end of a data block"));
        }
        src.read(self.pos + offset, len)
    }
}
