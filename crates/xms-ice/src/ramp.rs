//! Ramps: keys of a value along 0 to 1, and how a ramp goes between them. A
//! float ramp, or a colour ramp (RGBA, linear). What ICE's Ramp node runs
//! (`Op::Ramp`), and what Imago's ramp widget edits (MaterialX's `ramp` too).

/// How a ramp goes from one key to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Interp {
    /// The key's value until the next key.
    Step,
    #[default]
    Linear,
    /// Smoothstep between the two keys (MaterialX's `smooth`).
    Smooth,
    /// Catmull-Rom through the keys around.
    Cubic,
}

impl Interp {
    pub fn label(self) -> &'static str {
        match self {
            Interp::Step => "Step",
            Interp::Linear => "Linear",
            Interp::Smooth => "Smooth",
            Interp::Cubic => "Cubic",
        }
    }
}

/// What a ramp holds: a float, or a colour (RGBA, linear).
pub trait RampValue: Copy + PartialEq {
    fn add(a: Self, b: Self) -> Self;
    fn sub(a: Self, b: Self) -> Self;
    fn scale(a: Self, s: f32) -> Self;
    fn mix(a: Self, b: Self, f: f32) -> Self {
        Self::add(a, Self::scale(Self::sub(b, a), f))
    }
}

impl RampValue for f32 {
    fn add(a: f32, b: f32) -> f32 {
        a + b
    }
    fn sub(a: f32, b: f32) -> f32 {
        a - b
    }
    fn scale(a: f32, s: f32) -> f32 {
        a * s
    }
}

impl RampValue for [f32; 4] {
    fn add(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|i| a[i] + b[i])
    }
    fn sub(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        std::array::from_fn(|i| a[i] - b[i])
    }
    fn scale(a: [f32; 4], s: f32) -> [f32; 4] {
        std::array::from_fn(|i| a[i] * s)
    }
}

/// A key: where along the ramp (0 to 1), and its value.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Key<T> {
    pub pos: f32,
    pub value: T,
}

/// Keys, in any order, and how the ramp goes between them.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Ramp<T> {
    pub keys: Vec<Key<T>>,
    pub interp: Interp,
}

impl<T: RampValue> Ramp<T> {
    /// The keys by position.
    pub fn sorted(&self) -> Vec<Key<T>> {
        let mut k = self.keys.clone();
        k.sort_by(|a, b| a.pos.total_cmp(&b.pos));
        k
    }

    /// The value at `t`: the first key's before it, the last's after.
    pub fn eval(&self, t: f32) -> Option<T> {
        eval_sorted(&self.sorted(), self.interp, t)
    }
}

/// The value at `t` of keys in order (`Ramp::sorted`): for many values of one
/// ramp, sorted once.
pub fn eval_sorted<T: RampValue>(k: &[Key<T>], interp: Interp, t: f32) -> Option<T> {
    let (first, last) = (k.first()?, k.last()?);
    if t <= first.pos {
        return Some(first.value);
    }
    if t >= last.pos {
        return Some(last.value);
    }
    let i = k.windows(2).position(|w| t >= w[0].pos && t < w[1].pos)?;
    let (a, b) = (k[i], k[i + 1]);
    let span = b.pos - a.pos;
    let f = if span > 0.0 { (t - a.pos) / span } else { 1.0 };
    Some(match interp {
        Interp::Step => a.value,
        Interp::Linear => T::mix(a.value, b.value, f),
        Interp::Smooth => T::mix(a.value, b.value, f * f * (3.0 - 2.0 * f)),
        Interp::Cubic => {
            // Hermite with Catmull-Rom tangents; at an end, its key stands
            // for the one beyond.
            let before = if i > 0 { k[i - 1] } else { a };
            let after = k.get(i + 2).copied().unwrap_or(b);
            let m0 = T::scale(T::sub(b.value, before.value), 0.5);
            let m1 = T::scale(T::sub(after.value, a.value), 0.5);
            let (f2, f3) = (f * f, f * f * f);
            let h00 = 2.0 * f3 - 3.0 * f2 + 1.0;
            let h10 = f3 - 2.0 * f2 + f;
            let h01 = -2.0 * f3 + 3.0 * f2;
            let h11 = f3 - f2;
            T::add(
                T::add(T::scale(a.value, h00), T::scale(m0, h10)),
                T::add(T::scale(b.value, h01), T::scale(m1, h11)),
            )
        }
    })
}

/// What an ICE Ramp node holds: a float ramp, giving a float, or a colour ramp,
/// giving a colour (RGB; its keys' alpha is not given).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RampOp {
    Float(Ramp<f32>),
    Colour(Ramp<[f32; 4]>),
}

impl RampOp {
    /// What it gives.
    pub fn output(&self) -> crate::Ty {
        match self {
            RampOp::Float(_) => crate::Ty::Float,
            RampOp::Colour(_) => crate::Ty::Vec3,
        }
    }

    /// A float ramp from 0 to 1.
    pub fn float() -> RampOp {
        RampOp::Float(Ramp {
            keys: vec![Key { pos: 0.0, value: 0.0 }, Key { pos: 1.0, value: 1.0 }],
            interp: Interp::Linear,
        })
    }

    /// A colour ramp from black to white.
    pub fn colour() -> RampOp {
        RampOp::Colour(Ramp {
            keys: vec![
                Key {
                    pos: 0.0,
                    value: [0.0, 0.0, 0.0, 1.0],
                },
                Key {
                    pos: 1.0,
                    value: [1.0; 4],
                },
            ],
            interp: Interp::Linear,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(interp: Interp) -> Ramp<f32> {
        Ramp {
            keys: vec![
                Key { pos: 1.0, value: 1.0 },
                Key { pos: 0.0, value: 0.0 },
                Key { pos: 0.5, value: 0.2 },
            ],
            interp,
        }
    }

    #[test]
    fn keys_are_read_in_order_whatever_their_order() {
        let r = ramp(Interp::Linear);
        assert_eq!(r.eval(-1.0), Some(0.0));
        assert_eq!(r.eval(2.0), Some(1.0));
        assert!((r.eval(0.25).unwrap() - 0.1).abs() < 1e-6);
        assert!((r.eval(0.75).unwrap() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn step_holds_smooth_eases_cubic_passes_the_keys() {
        assert_eq!(ramp(Interp::Step).eval(0.49), Some(0.0));
        assert_eq!(ramp(Interp::Step).eval(0.5), Some(0.2));
        // Smoothstep: a quarter of the way, 0.15625 of the value.
        let s = ramp(Interp::Smooth).eval(0.125).unwrap();
        assert!((s - 0.2 * 0.15625).abs() < 1e-6, "{s}");
        let c = ramp(Interp::Cubic);
        for (t, v) in [(0.0, 0.0), (0.5, 0.2), (1.0, 1.0)] {
            assert!((c.eval(t).unwrap() - v).abs() < 1e-5);
        }
        assert!(c.eval(0.25).unwrap() > 0.0 && c.eval(0.75).unwrap() > 0.2);
    }

    #[test]
    fn sorted_once_reads_as_the_ramp() {
        let r = ramp(Interp::Smooth);
        let k = r.sorted();
        for t in [0.0, 0.1, 0.33, 0.5, 0.8, 1.0] {
            assert_eq!(eval_sorted(&k, r.interp, t), r.eval(t));
        }
        assert_eq!(RampOp::float().output(), crate::Ty::Float);
        assert_eq!(RampOp::colour().output(), crate::Ty::Vec3);
    }

    #[test]
    fn a_colour_ramp_mixes_each_channel() {
        let r = Ramp {
            keys: vec![
                Key {
                    pos: 0.0,
                    value: [0.0, 0.0, 1.0, 1.0],
                },
                Key {
                    pos: 1.0,
                    value: [1.0, 0.0, 0.0, 0.0],
                },
            ],
            interp: Interp::Linear,
        };
        assert_eq!(r.eval(0.5), Some([0.5, 0.0, 0.5, 0.5]));
        assert_eq!(
            Ramp::<f32> {
                keys: vec![],
                interp: Interp::Linear
            }
            .eval(0.5),
            None
        );
    }
}
