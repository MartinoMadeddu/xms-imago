//! Property values: Alembic's plain old data types, read from a sample's
//! bytes into typed vectors.

use crate::Error;

/// A property's element type, as Alembic numbers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pod {
    Bool,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F16,
    F32,
    F64,
    String,
    WString,
}

impl Pod {
    pub(crate) fn from_code(c: u32) -> Option<Pod> {
        use Pod::*;
        Some(match c {
            0 => Bool,
            1 => U8,
            2 => I8,
            3 => U16,
            4 => I16,
            5 => U32,
            6 => I32,
            7 => U64,
            8 => I64,
            9 => F16,
            10 => F32,
            11 => F64,
            12 => String,
            13 => WString,
            _ => return None,
        })
    }

    /// Bytes of one element (strings have no fixed size: 0).
    pub fn size(self) -> usize {
        use Pod::*;
        match self {
            Bool | U8 | I8 => 1,
            U16 | I16 | F16 => 2,
            U32 | I32 | F32 => 4,
            U64 | I64 | F64 => 8,
            String | WString => 0,
        }
    }
}

/// A sample's values, flat: `extent` of them make one element (3 for a
/// point, 16 for a matrix).
#[derive(Clone, Debug, PartialEq)]
pub enum Values {
    Bool(Vec<bool>),
    U8(Vec<u8>),
    I8(Vec<i8>),
    U16(Vec<u16>),
    I16(Vec<i16>),
    U32(Vec<u32>),
    I32(Vec<i32>),
    U64(Vec<u64>),
    I64(Vec<i64>),
    /// Halves, read as `f32`.
    F16(Vec<f32>),
    F32(Vec<f32>),
    F64(Vec<f64>),
    String(Vec<String>),
}

impl Values {
    /// How many values (not elements).
    pub fn len(&self) -> usize {
        use Values::*;
        match self {
            Bool(v) => v.len(),
            U8(v) => v.len(),
            I8(v) => v.len(),
            U16(v) => v.len(),
            I16(v) => v.len(),
            U32(v) => v.len(),
            I32(v) => v.len(),
            U64(v) => v.len(),
            I64(v) => v.len(),
            F16(v) | F32(v) => v.len(),
            F64(v) => v.len(),
            String(v) => v.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every value as an `f64` (text: none).
    pub fn to_f64(&self) -> Vec<f64> {
        use Values::*;
        match self {
            Bool(v) => v.iter().map(|&x| if x { 1.0 } else { 0.0 }).collect(),
            U8(v) => v.iter().map(|&x| x as f64).collect(),
            I8(v) => v.iter().map(|&x| x as f64).collect(),
            U16(v) => v.iter().map(|&x| x as f64).collect(),
            I16(v) => v.iter().map(|&x| x as f64).collect(),
            U32(v) => v.iter().map(|&x| x as f64).collect(),
            I32(v) => v.iter().map(|&x| x as f64).collect(),
            U64(v) => v.iter().map(|&x| x as f64).collect(),
            I64(v) => v.iter().map(|&x| x as f64).collect(),
            F16(v) | F32(v) => v.iter().map(|&x| x as f64).collect(),
            F64(v) => v.clone(),
            String(_) => vec![],
        }
    }

    /// Every value as an `f32`.
    pub fn to_f32(&self) -> Vec<f32> {
        match self {
            Values::F16(v) | Values::F32(v) => v.clone(),
            other => other.to_f64().into_iter().map(|x| x as f32).collect(),
        }
    }

    /// Every value as an `i64` (fractions dropped).
    pub fn to_i64(&self) -> Vec<i64> {
        use Values::*;
        match self {
            I32(v) => v.iter().map(|&x| x as i64).collect(),
            I64(v) => v.clone(),
            U64(v) => v.iter().map(|&x| x as i64).collect(),
            other => other.to_f64().into_iter().map(|x| x as i64).collect(),
        }
    }

    /// Every value as an `i32`.
    pub fn to_i32(&self) -> Vec<i32> {
        match self {
            Values::I32(v) => v.clone(),
            other => other.to_i64().into_iter().map(|x| x as i32).collect(),
        }
    }

    /// The text values.
    pub fn strings(&self) -> &[String] {
        match self {
            Values::String(v) => v,
            _ => &[],
        }
    }
}

/// Elements of `N` values from flat ones: points from their coordinates.
pub fn chunked<T: Copy + Default, const N: usize>(flat: &[T]) -> Vec<[T; N]> {
    flat.chunks_exact(N)
        .map(|c| {
            let mut a = [T::default(); N];
            a.copy_from_slice(c);
            a
        })
        .collect()
}

/// A half-precision float as an `f32`.
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = (bits >> 10) & 0x1f;
    let frac = (bits & 0x3ff) as f32;
    match exp {
        0 => sign * frac * 2f32.powi(-24),
        31 if frac == 0.0 => sign * f32::INFINITY,
        31 => f32::NAN,
        e => sign * (1.0 + frac / 1024.0) * 2f32.powi(e as i32 - 15),
    }
}

/// The values of `bytes` (a sample's data after its 16-byte key) of `pod`.
pub(crate) fn decode(pod: Pod, bytes: &[u8]) -> Result<Values, Error> {
    macro_rules! nums {
        ($t:ty, $n:expr) => {
            bytes
                .chunks_exact($n)
                .map(|c| <$t>::from_le_bytes(c.try_into().expect("element bytes")))
                .collect()
        };
    }
    Ok(match pod {
        Pod::Bool => Values::Bool(bytes.iter().map(|&b| b != 0).collect()),
        Pod::U8 => Values::U8(bytes.to_vec()),
        Pod::I8 => Values::I8(bytes.iter().map(|&b| b as i8).collect()),
        Pod::U16 => Values::U16(nums!(u16, 2)),
        Pod::I16 => Values::I16(nums!(i16, 2)),
        Pod::U32 => Values::U32(nums!(u32, 4)),
        Pod::I32 => Values::I32(nums!(i32, 4)),
        Pod::U64 => Values::U64(nums!(u64, 8)),
        Pod::I64 => Values::I64(nums!(i64, 8)),
        Pod::F16 => Values::F16(
            bytes
                .chunks_exact(2)
                .map(|c| half(u16::from_le_bytes([c[0], c[1]])))
                .collect(),
        ),
        Pod::F32 => Values::F32(nums!(f32, 4)),
        Pod::F64 => Values::F64(nums!(f64, 8)),
        // Each ended by a 0.
        Pod::String => Values::String(
            bytes
                .split(|&b| b == 0)
                .take(bytes.iter().filter(|&&b| b == 0).count())
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect(),
        ),
        // 32-bit characters, each string ended by a 0.
        Pod::WString => {
            let chars: Vec<u32> = nums!(u32, 4);
            let mut out = vec![];
            let mut cur = String::new();
            for c in chars {
                if c == 0 {
                    out.push(std::mem::take(&mut cur));
                } else {
                    cur.push(char::from_u32(c).unwrap_or('\u{fffd}'));
                }
            }
            Values::String(out)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_read_from_their_bytes() {
        let f: Vec<u8> = [1.5f32, -2.0].iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(decode(Pod::F32, &f).unwrap(), Values::F32(vec![1.5, -2.0]));
        assert_eq!(
            decode(Pod::String, b"ab\0\0c\0").unwrap(),
            Values::String(vec!["ab".into(), "".into(), "c".into()])
        );
        // 1.0, -2.0 and 0.5 as halves.
        let h: Vec<u8> = [0x3c00u16, 0xc000, 0x3800]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        assert_eq!(decode(Pod::F16, &h).unwrap(), Values::F16(vec![1.0, -2.0, 0.5]));
        assert_eq!(
            chunked::<f32, 3>(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]
        );
    }
}
