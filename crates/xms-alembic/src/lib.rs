//! Alembic (`.abc`) files, read in pure Rust.
//!
//! Alembic is written as Ogawa containers (`ogawa`): a tree of groups and data
//! blocks. On that, an archive (`Archive`) is a tree of objects (`Object`),
//! each with properties (`Compound`, `Property`) whose samples each have a
//! time (`TimeSampling`). What an object is comes from its schema (`geom`):
//! transforms, polygon meshes, subdivision surfaces, curves, points, cameras
//! and face sets, with their geometry parameters (normals, texture
//! coordinates, colours, widths, by scope and with their indices).
//!
//! ```no_run
//! let archive = xms_alembic::Archive::open("shot.abc")?;
//! for obj in archive.root()?.children()? {
//!     if let xms_alembic::Geom::PolyMesh(mesh) = xms_alembic::geom(&obj)? {
//!         let points = mesh.positions_at(0)?;
//!         println!("{}: {} points, {} samples", obj.path, points.len(), mesh.num_samples());
//!     }
//! }
//! # Ok::<(), xms_alembic::Error>(())
//! ```
//!
//! - **Read as asked:** an archive keeps its file open and reads the blocks a
//!   call asks for; nothing is read in full on opening but the archive's
//!   headers. `Archive::from_bytes` reads one held in memory.
//! - **Values as written:** points in the file's units and axes, matrices
//!   row-major with points as rows (as Alembic and USD write them), times in
//!   seconds. Turning them into a scene (USD's time codes, its schemas) is the
//!   reader's caller's: `xms-usd` does it for USD stages.
//! - **Not read:** the older HDF5 kind of Alembic file (before about 2013),
//!   NURBS patches and lights (named by `Geom::Other`), and a camera's film
//!   back operations.
//!
//! Its own crate, depending on nothing, so the renderer and the app can read
//! Alembic without USD. Written from the Alembic library's source (BSD 3) as
//! the reference for the format, with `ogawa-rs` looked at alongside.

mod archive;
mod geom;
mod metadata;
mod ogawa;
mod time;
mod value;

pub use archive::{Archive, Compound, Kind, Object, Property, PropertyHeader};
pub use geom::{
    geom, matrix_of, multiply, visibility, visibility_at, Camera, CameraSample, CurveBasis, CurveShape, CurveType,
    Curves, Extra, FaceSet, Geom, GeomParam, Matrix, Mesh, ParamSample, Points, Scope, SubD, Visibility, Xform,
    XformOp, IDENTITY,
};
pub use metadata::MetaData;
pub use time::TimeSampling;
pub use value::{chunked, Pod, Values};

/// Why a file could not be read.
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    /// The file could not be opened or read.
    Io(String),
    /// Not an Alembic file this reader knows.
    NotAlembic(String),
    /// Something in it is not as Alembic writes it.
    Corrupt(&'static str),
    /// A property a schema must have is not there.
    Missing(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::NotAlembic(e) => write!(f, "Not an Alembic file this reader knows: {e}"),
            Error::Corrupt(what) => write!(f, "Corrupt Alembic file: {what}"),
            Error::Missing(what) => write!(f, "Alembic schema incomplete: {what}"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests;
