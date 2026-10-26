//! MaterialX data types and the values written in a document.

/// A MaterialX data type, by its name in the specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Type {
    Boolean,
    Integer,
    Float,
    Color3,
    Color4,
    Vector2,
    Vector3,
    Vector4,
    Matrix33,
    Matrix44,
    String,
    Filename,
    Geomname,
    IntegerArray,
    FloatArray,
    Color3Array,
    Color4Array,
    Vector2Array,
    Vector3Array,
    Vector4Array,
    StringArray,
    GeomnameArray,
    /// Closures, the pieces shaders are made of (pbrlib).
    Bsdf,
    Edf,
    Vdf,
    SurfaceShader,
    DisplacementShader,
    VolumeShader,
    LightShader,
    Material,
    None,
}

impl Type {
    pub const ALL: [Type; 31] = [
        Type::Boolean,
        Type::Integer,
        Type::Float,
        Type::Color3,
        Type::Color4,
        Type::Vector2,
        Type::Vector3,
        Type::Vector4,
        Type::Matrix33,
        Type::Matrix44,
        Type::String,
        Type::Filename,
        Type::Geomname,
        Type::IntegerArray,
        Type::FloatArray,
        Type::Color3Array,
        Type::Color4Array,
        Type::Vector2Array,
        Type::Vector3Array,
        Type::Vector4Array,
        Type::StringArray,
        Type::GeomnameArray,
        Type::Bsdf,
        Type::Edf,
        Type::Vdf,
        Type::SurfaceShader,
        Type::DisplacementShader,
        Type::VolumeShader,
        Type::LightShader,
        Type::Material,
        Type::None,
    ];

    /// Its name in a document.
    pub fn name(self) -> &'static str {
        match self {
            Type::Boolean => "boolean",
            Type::Integer => "integer",
            Type::Float => "float",
            Type::Color3 => "color3",
            Type::Color4 => "color4",
            Type::Vector2 => "vector2",
            Type::Vector3 => "vector3",
            Type::Vector4 => "vector4",
            Type::Matrix33 => "matrix33",
            Type::Matrix44 => "matrix44",
            Type::String => "string",
            Type::Filename => "filename",
            Type::Geomname => "geomname",
            Type::IntegerArray => "integerarray",
            Type::FloatArray => "floatarray",
            Type::Color3Array => "color3array",
            Type::Color4Array => "color4array",
            Type::Vector2Array => "vector2array",
            Type::Vector3Array => "vector3array",
            Type::Vector4Array => "vector4array",
            Type::StringArray => "stringarray",
            Type::GeomnameArray => "geomnamearray",
            Type::Bsdf => "BSDF",
            Type::Edf => "EDF",
            Type::Vdf => "VDF",
            Type::SurfaceShader => "surfaceshader",
            Type::DisplacementShader => "displacementshader",
            Type::VolumeShader => "volumeshader",
            Type::LightShader => "lightshader",
            Type::Material => "material",
            Type::None => "none",
        }
    }

    /// The type a document names.
    pub fn from_name(name: &str) -> Option<Type> {
        Type::ALL.into_iter().find(|t| t.name() == name)
    }

    /// How many floats a value of it holds: 1 for float, 3 for color3, 16 for
    /// matrix44; 0 for anything that is not floats.
    pub fn floats(self) -> usize {
        match self {
            Type::Float => 1,
            Type::Vector2 => 2,
            Type::Color3 | Type::Vector3 => 3,
            Type::Color4 | Type::Vector4 => 4,
            Type::Matrix33 => 9,
            Type::Matrix44 => 16,
            _ => 0,
        }
    }

    /// A colour (shown with a colour swatch).
    pub fn is_color(self) -> bool {
        matches!(self, Type::Color3 | Type::Color4)
    }

    /// A closure or a shader: what flows between shading nodes, not a value
    /// that can be typed in.
    pub fn is_shading(self) -> bool {
        matches!(
            self,
            Type::Bsdf
                | Type::Edf
                | Type::Vdf
                | Type::SurfaceShader
                | Type::DisplacementShader
                | Type::VolumeShader
                | Type::LightShader
                | Type::Material
        )
    }

    /// A list of values.
    pub fn is_array(self) -> bool {
        self.name().ends_with("array")
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A value of a given type, as a document writes it.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Boolean(bool),
    Integer(i32),
    /// float, colorN, vectorN, matrixNN: its floats, in order.
    Floats(Vec<f32>),
    /// string, filename, geomname.
    String(String),
    IntegerArray(Vec<i32>),
    /// floatarray and the arrays of colours and vectors, flattened.
    FloatArray(Vec<f32>),
    StringArray(Vec<String>),
}

impl Value {
    /// The value `text` is for `ty`. A closure, shader or `none` has no value;
    /// text that does not read as the type is `None` too.
    pub fn parse(ty: Type, text: &str) -> Option<Value> {
        let text = text.trim();
        match ty {
            Type::Boolean => match text {
                "true" => Some(Value::Boolean(true)),
                "false" => Some(Value::Boolean(false)),
                _ => None,
            },
            Type::Integer => text.parse().ok().map(Value::Integer),
            t if t.floats() > 0 => {
                let v: Option<Vec<f32>> = list(text).iter().map(|x| x.parse().ok()).collect();
                v.filter(|v| v.len() == t.floats()).map(Value::Floats)
            }
            Type::String | Type::Filename | Type::Geomname => Some(Value::String(text.to_string())),
            Type::IntegerArray => list(text)
                .iter()
                .map(|x| x.parse().ok())
                .collect::<Option<Vec<i32>>>()
                .map(Value::IntegerArray),
            Type::FloatArray
            | Type::Color3Array
            | Type::Color4Array
            | Type::Vector2Array
            | Type::Vector3Array
            | Type::Vector4Array => list(text)
                .iter()
                .map(|x| x.parse().ok())
                .collect::<Option<Vec<f32>>>()
                .map(Value::FloatArray),
            Type::StringArray | Type::GeomnameArray => {
                Some(Value::StringArray(list(text).into_iter().map(String::from).collect()))
            }
            _ => None,
        }
    }

    /// A single float, if it is one.
    pub fn as_float(&self) -> Option<f32> {
        match self {
            Value::Floats(v) if v.len() == 1 => Some(v[0]),
            Value::Integer(i) => Some(*i as f32),
            _ => None,
        }
    }

    /// The value as a document writes it: `0.8, 0.8, 0.8`.
    pub fn to_text(&self) -> String {
        let join = |v: &[String]| v.join(", ");
        match self {
            Value::Boolean(b) => b.to_string(),
            Value::Integer(i) => i.to_string(),
            Value::Floats(v) | Value::FloatArray(v) => join(&v.iter().map(|x| float_text(*x)).collect::<Vec<_>>()),
            Value::String(s) => s.clone(),
            Value::IntegerArray(v) => join(&v.iter().map(|x| x.to_string()).collect::<Vec<_>>()),
            Value::StringArray(v) => join(v),
        }
    }
}

/// A float as a document writes it: always with a decimal point.
fn float_text(x: f32) -> String {
    let s = format!("{x}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

/// The comma-separated items of a value.
fn list(s: &str) -> Vec<&str> {
    if s.is_empty() {
        vec![]
    } else {
        s.split(',').map(str::trim).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_has_one_name() {
        for t in Type::ALL {
            assert_eq!(Type::from_name(t.name()), Some(t));
        }
        assert_eq!(Type::from_name("BSDF"), Some(Type::Bsdf));
        assert_eq!(Type::from_name("bsdf"), None);
    }

    #[test]
    fn values_read_as_their_type() {
        assert_eq!(
            Value::parse(Type::Color3, "0.8, 0.8, 0.8"),
            Some(Value::Floats(vec![0.8, 0.8, 0.8]))
        );
        // Written without spaces too (`uimin="0,0,0"`).
        assert_eq!(
            Value::parse(Type::Color3, "0,0,1"),
            Some(Value::Floats(vec![0.0, 0.0, 1.0]))
        );
        assert_eq!(Value::parse(Type::Color3, "0.8, 0.8"), None);
        assert_eq!(Value::parse(Type::Float, "0"), Some(Value::Floats(vec![0.0])));
        assert_eq!(Value::parse(Type::Integer, "3"), Some(Value::Integer(3)));
        assert_eq!(Value::parse(Type::Boolean, "true"), Some(Value::Boolean(true)));
        assert_eq!(Value::parse(Type::Filename, ""), Some(Value::String(String::new())));
        assert_eq!(Value::parse(Type::FloatArray, ""), Some(Value::FloatArray(vec![])));
        assert_eq!(Value::parse(Type::SurfaceShader, ""), None);
        assert_eq!(
            Value::parse(Type::Matrix33, "1,0,0, 0,1,0, 0,0,1").map(|v| v.to_text()),
            Some("1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0".to_string())
        );
    }
}
