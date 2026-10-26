//! The colours of data types, on ports and wires, the same in every graph of
//! XMS (ICE, MaterialX, the scene graph's material sockets). A graph maps its
//! own types to a `DataType` and takes the colour from here, so a float is the
//! same teal-green wherever it is wired. See `docs/interface.md`, "Data type
//! colours".
//!
//! Close to Softimage ICE's, with Houdini VOPs' float and string colours. The
//! colours do not change with the light or dark theme: they are read against
//! the grey of the graphs in both.

use egui::Color32;

/// What a port carries, as far as its colour goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataType {
    /// A number with a fraction (scalar, float).
    Float,
    /// A whole number.
    Integer,
    /// True or false: the integers' blue.
    Boolean,
    /// Two or three numbers: a vector, a position, texture coordinates.
    Vector,
    /// Four numbers: a 4-vector, a quaternion, a rotation.
    Vector4,
    /// A colour (RGB or RGBA).
    Colour,
    /// A 3×3 or 4×4 matrix.
    Matrix,
    /// Text: a string, a file name, a token.
    String,
    /// A material (the scene graph's material sockets, MaterialX's `material`)
    /// and the shaders that make one.
    Material,
    /// A BSDF, EDF or VDF: what a shader is built from.
    Closure,
    /// The order things run in (ICE's Execute), and what is not known yet.
    Other,
}

/// Every data type, in the order the docs list them.
#[cfg(test)]
pub const ALL: [DataType; 11] = [
    DataType::Float,
    DataType::Integer,
    DataType::Boolean,
    DataType::Vector,
    DataType::Vector4,
    DataType::Colour,
    DataType::Matrix,
    DataType::String,
    DataType::Material,
    DataType::Closure,
    DataType::Other,
];

impl DataType {
    /// Its colour on ports and wires.
    pub fn colour(self) -> Color32 {
        let (r, g, b) = match self {
            DataType::Float => (85, 210, 197),    // teal-green: Houdini VOPs' float
            DataType::Integer => (80, 140, 230),  // blue
            DataType::Boolean => (80, 140, 230),  // blue, as integers
            DataType::Vector => (235, 225, 60),   // yellow
            DataType::Vector4 => (175, 150, 30),  // dark yellow
            DataType::Colour => (240, 130, 60),   // orange
            DataType::Matrix => (140, 95, 190),   // purple, as materials: the light purple was hard to see
            DataType::String => (210, 165, 125),  // Houdini VOPs' OpInput, a little more saturated
            DataType::Material => (140, 95, 190), // purple
            DataType::Closure => (225, 120, 205), // pink-magenta
            DataType::Other => (85, 85, 85),      // dark grey
        };
        Color32::from_rgb(r, g, b)
    }

    /// The name the docs give it.
    #[cfg(test)]
    pub fn name(self) -> &'static str {
        match self {
            DataType::Float => "Float",
            DataType::Integer => "Integer",
            DataType::Boolean => "Boolean",
            DataType::Vector => "Vector (2D, 3D), position",
            DataType::Vector4 => "Vector 4, quaternion, rotation",
            DataType::Colour => "Colour",
            DataType::Matrix => "Matrix",
            DataType::String => "String, file name",
            DataType::Material => "Material, shader",
            DataType::Closure => "BSDF, EDF, VDF",
            DataType::Other => "Execute, not known yet",
        }
    }
}

/// An array's ports and wires in ICE, whatever it holds (its port's tooltip
/// says what): the closures' pink-magenta, since no ICE graph carries a BSDF,
/// EDF or VDF.
pub fn array() -> Color32 {
    DataType::Closure.colour()
}

/// A port's colour when connected: a little lighter.
pub fn connected(c: Color32) -> Color32 {
    mix(c, Color32::WHITE, 0.25)
}

/// A port's colour under the mouse: lighter still.
pub fn hovered(c: Color32) -> Color32 {
    mix(c, Color32::WHITE, 0.45)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types meant to read apart do: no two colours closer than a step a glance
    /// tells apart, apart from integers and booleans, and matrices and
    /// materials, which share one (no graph carries both).
    #[test]
    fn the_colours_read_apart() {
        let d = |a: Color32, b: Color32| {
            let (dr, dg, db) = (
                a.r() as f32 - b.r() as f32,
                a.g() as f32 - b.g() as f32,
                a.b() as f32 - b.b() as f32,
            );
            (dr * dr + dg * dg + db * db).sqrt()
        };
        for (i, a) in ALL.iter().enumerate() {
            for b in &ALL[i + 1..] {
                if matches!(
                    (*a, *b),
                    (DataType::Integer, DataType::Boolean) | (DataType::Matrix, DataType::Material)
                ) {
                    continue;
                }
                assert!(d(a.colour(), b.colour()) > 45.0, "{a:?} and {b:?} are too close");
            }
        }
        assert_eq!(DataType::Integer.colour(), DataType::Boolean.colour());
        assert_eq!(DataType::Matrix.colour(), DataType::Material.colour());
        // An ICE array's pink reads apart from every type it can hold
        // (closures, which take the same, are never in an ICE graph).
        for t in ALL
            .iter()
            .filter(|t| !matches!(**t, DataType::Other | DataType::Closure))
        {
            assert!(d(array(), t.colour()) > 45.0, "arrays and {t:?} are too close");
        }
    }
}
