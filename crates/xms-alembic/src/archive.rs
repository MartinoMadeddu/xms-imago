//! An Alembic archive as written by AbcCoreOgawa: a tree of objects, each with
//! a compound of properties, each property scalar (one element a sample),
//! array (any number a sample) or compound (more properties).
//!
//! - **The archive's top group:** its Alembic version, its file version, the
//!   root object, its metadata, its time samplings, and metadata used by many
//!   headers (indexed once, named by an index after).
//! - **An object's group:** its properties (a compound) first, then each child
//!   object's group, then its children's headers (name and metadata).
//! - **A compound's group:** each property's group, then their headers (kind,
//!   type, time sampling, which samples differ, name, metadata).
//! - **A property's group:** its samples, each written once: a scalar's one
//!   block each, an array's two (the values, then their dimensions). Samples
//!   that repeat the one before at the start or end are not written again.
//!   Each block starts with a 16-byte key (a hash of the values).

use std::path::Path;
use std::sync::Arc;

use crate::metadata::MetaData;
use crate::ogawa::{self, Data, Group, Source};
use crate::time::{read_time_samplings, TimeSampling};
use crate::value::{decode, Pod, Values};
use crate::Error;

/// An opened `.abc` file.
pub struct Archive {
    src: Source,
    top: Group,
    /// Each with the most samples a property using it has.
    samplings: Vec<(TimeSampling, u32)>,
    /// Metadata a header names by its index (0: none).
    indexed: Vec<MetaData>,
    metadata: MetaData,
    /// The Alembic library's version that wrote it (10709 is 1.7.9).
    pub library_version: i32,
}

impl Archive {
    /// Open `path`, reading its bytes as they are asked for.
    pub fn open(path: impl AsRef<Path>) -> Result<Archive, Error> {
        Archive::from_source(Source::open(path.as_ref())?)
    }

    /// An archive held in memory.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Archive, Error> {
        Archive::from_source(Source::Bytes(Arc::from(bytes)))
    }

    fn from_source(src: Source) -> Result<Archive, Error> {
        let top = Group::read(&src, ogawa::top_group(&src)?)?;
        let layout = top.len() > 5
            && top.is_data(0)
            && top.is_data(1)
            && top.is_group(2)
            && top.is_data(3)
            && top.is_data(4)
            && top.is_data(5);
        if !layout {
            return Err(Error::NotAlembic("an Ogawa file, but not an Alembic archive".into()));
        }
        let int = |d: Data| -> Result<i32, Error> {
            let b = d.bytes(&src)?;
            Ok(if b.len() == 4 {
                i32::from_le_bytes(b.try_into().expect("4 bytes"))
            } else {
                -1
            })
        };
        let version = int(top.data(&src, 0)?)?;
        if !(0..=1).contains(&version) {
            return Err(Error::NotAlembic(format!(
                "Alembic Ogawa version {version} is not known"
            )));
        }
        let library_version = int(top.data(&src, 1)?)?;
        let metadata = MetaData::parse(&String::from_utf8_lossy(&top.data(&src, 3)?.bytes(&src)?));
        let mut samplings = read_time_samplings(&top.data(&src, 4)?.bytes(&src)?)?;
        if samplings.is_empty() {
            samplings.push((TimeSampling::default(), 0));
        }
        let indexed = read_indexed_metadata(&top.data(&src, 5)?.bytes(&src)?)?;
        Ok(Archive {
            src,
            top,
            samplings,
            indexed,
            metadata,
            library_version,
        })
    }

    /// The archive's metadata: the application that wrote it
    /// (`_ai_Application`), its frames per second (`_ai_DCC_FPS`), a
    /// description (`_ai_Description`)…
    pub fn metadata(&self) -> &MetaData {
        &self.metadata
    }

    /// Time sampling `i`, as properties name them (the default when there is
    /// no such one).
    pub fn time_sampling(&self, i: u32) -> &TimeSampling {
        static DEFAULT: std::sync::OnceLock<TimeSampling> = std::sync::OnceLock::new();
        self.samplings
            .get(i as usize)
            .map(|(t, _)| t)
            .unwrap_or_else(|| DEFAULT.get_or_init(TimeSampling::default))
    }

    /// How many time samplings the archive has.
    pub fn time_sampling_count(&self) -> usize {
        self.samplings.len()
    }

    /// The first and last sample times, in seconds, of the properties that
    /// change over time; `None` when nothing does.
    pub fn time_range(&self) -> Option<(f64, f64)> {
        let mut range: Option<(f64, f64)> = None;
        for (t, max) in &self.samplings {
            if *max < 2 {
                continue;
            }
            let (a, b) = (t.time(0), t.time(*max as usize - 1));
            range = Some(match range {
                Some((lo, hi)) => (lo.min(a), hi.max(b)),
                None => (a, b),
            });
        }
        range
    }

    /// The root object (`/`), which holds the top objects.
    pub fn root(&self) -> Result<Object<'_>, Error> {
        Object::read(self, self.top.group(&self.src, 2)?, "ABC", "/", self.metadata.clone())
    }

    /// The object at `path` (`/car/body`), when there is one.
    pub fn object(&self, path: &str) -> Result<Option<Object<'_>>, Error> {
        let mut at = self.root()?;
        for name in path.split('/').filter(|s| !s.is_empty()) {
            match at.child(name)? {
                Some(c) => at = c,
                None => return Ok(None),
            }
        }
        Ok(Some(at))
    }

    fn meta(&self, index: u8, text: Option<String>) -> MetaData {
        match text {
            Some(t) => MetaData::parse(&t),
            None => self.indexed.get(index as usize).cloned().unwrap_or_default(),
        }
    }
}

fn read_indexed_metadata(bytes: &[u8]) -> Result<Vec<MetaData>, Error> {
    let mut out = vec![MetaData::default()];
    let mut pos = 0;
    while pos < bytes.len() {
        let n = bytes[pos] as usize;
        pos += 1;
        if pos + n > bytes.len() {
            return Err(Error::Corrupt("the archive's indexed metadata"));
        }
        out.push(MetaData::parse(&String::from_utf8_lossy(&bytes[pos..pos + n])));
        pos += n;
    }
    Ok(out)
}

/// A child object as its parent lists it.
#[derive(Clone, Debug)]
struct ObjectHeader {
    name: String,
    metadata: MetaData,
}

/// An object of the archive: a transform, a mesh, a camera, a group…, by its
/// schema (`schema` in its metadata).
#[derive(Clone)]
pub struct Object<'a> {
    archive: &'a Archive,
    group: Group,
    pub name: String,
    /// Its full path: `/car/body`.
    pub path: String,
    pub metadata: MetaData,
    children: Vec<ObjectHeader>,
}

impl std::fmt::Debug for Object<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Object({}, {:?})", self.path, self.schema())
    }
}

impl<'a> Object<'a> {
    fn read(
        archive: &'a Archive,
        group: Group,
        name: &str,
        path: &str,
        metadata: MetaData,
    ) -> Result<Object<'a>, Error> {
        let mut children = vec![];
        let last = group.len().checked_sub(1);
        if let Some(last) = last.filter(|&l| group.is_data(l)) {
            let data = group.data(&archive.src, last)?;
            // The last 32 bytes are hashes.
            if data.size > 32 {
                let buf = data.slice(&archive.src, 0, data.size - 32)?;
                children = read_object_headers(archive, &buf)?;
            }
        }
        Ok(Object {
            archive,
            group,
            name: name.to_string(),
            path: path.to_string(),
            metadata,
            children,
        })
    }

    pub fn archive(&self) -> &'a Archive {
        self.archive
    }

    /// Its schema: `AbcGeom_Xform_v3`, `AbcGeom_PolyMesh_v1`…; `None` for a
    /// plain object.
    pub fn schema(&self) -> Option<&str> {
        self.metadata.get("schema")
    }

    /// The names of its children, in their order.
    pub fn child_names(&self) -> Vec<&str> {
        self.children.iter().map(|c| c.name.as_str()).collect()
    }

    /// Its children, in their order.
    pub fn children(&self) -> Result<Vec<Object<'a>>, Error> {
        (0..self.children.len()).map(|i| self.child_at(i)).collect()
    }

    /// Its child named `name`.
    pub fn child(&self, name: &str) -> Result<Option<Object<'a>>, Error> {
        match self.children.iter().position(|c| c.name == name) {
            Some(i) => self.child_at(i).map(Some),
            None => Ok(None),
        }
    }

    fn child_at(&self, i: usize) -> Result<Object<'a>, Error> {
        let h = &self.children[i];
        let path = if self.path == "/" {
            format!("/{}", h.name)
        } else {
            format!("{}/{}", self.path, h.name)
        };
        let group = self.group.group(&self.archive.src, i + 1)?;
        Object::read(self.archive, group, &h.name, &path, h.metadata.clone())
    }

    /// Its properties: its schema's compound (`.geom`, `.xform`), visibility,
    /// `.instanceSource`…
    pub fn properties(&self) -> Result<Compound<'a>, Error> {
        if self.group.is_group(0) {
            Compound::read(self.archive, self.group.group(&self.archive.src, 0)?)
        } else {
            Ok(Compound::empty(self.archive))
        }
    }

    /// An instance: it stands for another object (`instance_source`), its
    /// children shared.
    pub fn is_instance(&self) -> bool {
        self.metadata.get("isInstance") == Some("1")
    }

    /// The path of the object an instance stands for.
    pub fn instance_source(&self) -> Result<Option<String>, Error> {
        if !self.is_instance() {
            return Ok(None);
        }
        let props = self.properties()?;
        let Some(p) = props.property(".instanceSource")? else {
            return Ok(None);
        };
        Ok(p.sample(0)?.strings().first().cloned())
    }
}

fn read_object_headers(archive: &Archive, buf: &[u8]) -> Result<Vec<ObjectHeader>, Error> {
    let bad = || Error::Corrupt("an object's children's headers");
    let mut out = vec![];
    let mut pos = 0;
    let u32_at = |pos: usize| -> Result<u32, Error> {
        buf.get(pos..pos + 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")))
            .ok_or(Error::Corrupt("an object's children's headers"))
    };
    while pos < buf.len() {
        let n = u32_at(pos)? as usize;
        pos += 4;
        if n == 0 || pos + n + 1 > buf.len() {
            return Err(bad());
        }
        let name = String::from_utf8_lossy(&buf[pos..pos + n]).into_owned();
        pos += n;
        let index = buf[pos];
        pos += 1;
        let text = if index == 0xff {
            let m = u32_at(pos)? as usize;
            pos += 4;
            if pos + m > buf.len() {
                return Err(bad());
            }
            let t = String::from_utf8_lossy(&buf[pos..pos + m]).into_owned();
            pos += m;
            Some(t)
        } else {
            None
        };
        out.push(ObjectHeader {
            name,
            metadata: archive.meta(index, text),
        });
    }
    Ok(out)
}

/// What a property is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// More properties.
    Compound,
    /// One element a sample.
    Scalar,
    /// Any number of elements a sample.
    Array,
}

/// A property as its compound lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyHeader {
    pub name: String,
    pub kind: Kind,
    pub pod: Pod,
    /// Values an element has: 3 for a point, 16 for a matrix.
    pub extent: u8,
    pub metadata: MetaData,
    /// Its time sampling, by index in the archive.
    pub time_sampling: u32,
    /// How many samples it has.
    pub samples: u32,
    /// Samples before this one repeat the first; from the last on, they
    /// repeat the last.
    first_changed: u32,
    last_changed: u32,
}

impl PropertyHeader {
    /// Where sample `i` is written: samples that repeat the first or the last
    /// are not written again.
    fn stored(&self, i: u32) -> usize {
        let (first, last) = (self.first_changed, self.last_changed);
        if i < first || (first == 0 && last == 0) {
            0
        } else if i >= last {
            (last - first + 1) as usize
        } else {
            (i - first + 1) as usize
        }
    }
}

/// A compound property: its properties by name.
#[derive(Clone)]
pub struct Compound<'a> {
    archive: &'a Archive,
    group: Group,
    headers: Vec<PropertyHeader>,
}

impl<'a> Compound<'a> {
    fn empty(archive: &'a Archive) -> Compound<'a> {
        Compound {
            archive,
            group: Group::default(),
            headers: vec![],
        }
    }

    fn read(archive: &'a Archive, group: Group) -> Result<Compound<'a>, Error> {
        let mut headers = vec![];
        if let Some(last) = group.len().checked_sub(1).filter(|&l| group.is_data(l)) {
            let buf = group.data(&archive.src, last)?.bytes(&archive.src)?;
            headers = read_property_headers(archive, &buf)?;
        }
        Ok(Compound {
            archive,
            group,
            headers,
        })
    }

    /// Its properties' headers, in their order.
    pub fn headers(&self) -> &[PropertyHeader] {
        &self.headers
    }

    /// The header of its property `name`.
    pub fn header(&self, name: &str) -> Option<&PropertyHeader> {
        self.headers.iter().find(|h| h.name == name)
    }

    /// Its scalar or array property `name`.
    pub fn property(&self, name: &str) -> Result<Option<Property<'a>>, Error> {
        let Some(i) = self.headers.iter().position(|h| h.name == name) else {
            return Ok(None);
        };
        let header = self.headers[i].clone();
        if header.kind == Kind::Compound {
            return Ok(None);
        }
        Ok(Some(Property {
            archive: self.archive,
            group: self.group.group(&self.archive.src, i)?,
            header,
        }))
    }

    /// Its compound property `name`.
    pub fn compound(&self, name: &str) -> Result<Option<Compound<'a>>, Error> {
        match self.headers.iter().position(|h| h.name == name) {
            Some(i) if self.headers[i].kind == Kind::Compound => {
                Compound::read(self.archive, self.group.group(&self.archive.src, i)?).map(Some)
            }
            _ => Ok(None),
        }
    }
}

fn read_property_headers(archive: &Archive, buf: &[u8]) -> Result<Vec<PropertyHeader>, Error> {
    let bad = || Error::Corrupt("a compound's property headers");
    let mut out = vec![];
    let mut pos = 0;
    // A number in 1, 2 or 4 bytes, by the header's size hint.
    let sized = |hint: u32, pos: &mut usize| -> Result<u32, Error> {
        let n = [1, 2, 4][hint.min(2) as usize];
        let b = buf
            .get(*pos..*pos + n)
            .ok_or(Error::Corrupt("a compound's property headers"))?;
        *pos += n;
        Ok(match n {
            1 => b[0] as u32,
            2 => u16::from_le_bytes([b[0], b[1]]) as u32,
            _ => u32::from_le_bytes(b.try_into().expect("4 bytes")),
        })
    };
    while pos < buf.len() {
        let info = u32::from_le_bytes(buf.get(pos..pos + 4).ok_or_else(bad)?.try_into().expect("4 bytes"));
        pos += 4;
        let kind = match info & 0x3 {
            0 => Kind::Compound,
            1 => Kind::Scalar,
            _ => Kind::Array,
        };
        let hint = (info & 0xc) >> 2;
        let mut header = PropertyHeader {
            name: String::new(),
            kind,
            pod: Pod::U8,
            extent: 1,
            metadata: MetaData::default(),
            time_sampling: 0,
            samples: 0,
            first_changed: 0,
            last_changed: 0,
        };
        if kind != Kind::Compound {
            header.pod = Pod::from_code((info & 0xf0) >> 4).ok_or(Error::Corrupt("a property's type"))?;
            header.extent = ((info & 0xff000) >> 12) as u8;
            header.samples = sized(hint, &mut pos)?;
            if info & 0x200 != 0 {
                header.first_changed = sized(hint, &mut pos)?;
                header.last_changed = sized(hint, &mut pos)?;
            } else if info & 0x800 != 0 {
                (header.first_changed, header.last_changed) = (0, 0);
            } else {
                header.first_changed = 1;
                header.last_changed = header.samples.saturating_sub(1);
            }
            if info & 0x100 != 0 {
                header.time_sampling = sized(hint, &mut pos)?;
            }
        }
        let n = sized(hint, &mut pos)? as usize;
        if n == 0 || pos + n > buf.len() {
            return Err(bad());
        }
        header.name = String::from_utf8_lossy(&buf[pos..pos + n]).into_owned();
        pos += n;
        let index = ((info & 0xff0_0000) >> 20) as u8;
        let text = if index == 0xff {
            let m = sized(hint, &mut pos)? as usize;
            if pos + m > buf.len() {
                return Err(bad());
            }
            let t = String::from_utf8_lossy(&buf[pos..pos + m]).into_owned();
            pos += m;
            Some(t)
        } else {
            None
        };
        header.metadata = archive.meta(index, text);
        out.push(header);
    }
    Ok(out)
}

/// A scalar or array property: its samples and when they are.
#[derive(Clone)]
pub struct Property<'a> {
    archive: &'a Archive,
    group: Group,
    pub header: PropertyHeader,
}

impl std::fmt::Debug for Property<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Property({}, {:?} {:?}x{}, {} samples)",
            self.header.name, self.header.kind, self.header.pod, self.header.extent, self.header.samples
        )
    }
}

impl<'a> Property<'a> {
    /// How many samples it has.
    pub fn num_samples(&self) -> usize {
        self.header.samples as usize
    }

    /// The same at every sample.
    pub fn is_constant(&self) -> bool {
        self.header.samples <= 1 || (self.header.first_changed == 0 && self.header.last_changed == 0)
    }

    /// When sample `i` is, in seconds.
    pub fn time(&self, i: usize) -> f64 {
        self.archive.time_sampling(self.header.time_sampling).time(i)
    }

    /// When each sample is, in seconds.
    pub fn times(&self) -> Vec<f64> {
        (0..self.num_samples()).map(|i| self.time(i)).collect()
    }

    /// The sample at or before time `t` (seconds), and its index: the first
    /// before it, the last after.
    pub fn floor(&self, t: f64) -> usize {
        let n = self.num_samples();
        if n == 0 {
            return 0;
        }
        (0..n).rev().find(|&i| self.time(i) <= t + 1e-9).unwrap_or(0)
    }

    /// Sample `i`'s values, flat (`extent` of them an element). A sample
    /// past the last is the last.
    pub fn sample(&self, i: usize) -> Result<Values, Error> {
        let src = &self.archive.src;
        let i = (i.min(self.num_samples().saturating_sub(1))) as u32;
        let k = self.header.stored(i);
        let data = match self.header.kind {
            Kind::Array => self.group.data(src, 2 * k)?,
            _ => self.group.data(src, k)?,
        };
        // A 16-byte key, then the values.
        if data.size < 16 {
            return decode(self.header.pod, &[]);
        }
        let bytes = data.slice(src, 16, data.size - 16)?;
        decode(self.header.pod, &bytes)
    }

    /// An array sample's dimensions (its element count for a flat array).
    pub fn dims(&self, i: usize) -> Result<Vec<u64>, Error> {
        if self.header.kind != Kind::Array {
            return Ok(vec![1]);
        }
        let src = &self.archive.src;
        let i = (i.min(self.num_samples().saturating_sub(1))) as u32;
        let k = self.header.stored(i);
        let dims = self.group.data(src, 2 * k + 1)?;
        if dims.size == 0 {
            let data = self.group.data(src, 2 * k)?;
            let per = self.header.pod.size().max(1) * self.header.extent.max(1) as usize;
            return Ok(vec![data.size.saturating_sub(16) / per as u64]);
        }
        Ok(dims
            .bytes(src)?
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().expect("8 bytes")))
            .collect())
    }
}
