//! Keys on parameters: animation curves, as a channel per animated number.
//!
//! Any number among an operator's parameters can be keyed: a channel holds its
//! keys, and at each frame the operator cooks with the value the curve gives
//! there. A channel is found by its **path** in the operator's parameters as
//! they are saved: field names and indices joined by `/` (`translation/0` is a
//! Transform's translate X). So every operator, and every operator added later,
//! is animatable without code of its own.
//!
//! A curve between two keys is a cubic Bezier in (frame, value), as in the
//! curve editors of the usual 3D programs. Each side of a key has a tangent
//! type that sets its slope: Auto, Spline, Clamped, Linear, Flat, Stepped,
//! Plateau, or Fixed (dragged by hand). A channel is weighted or not: not
//! weighted, a handle is a third of the way to the next key; weighted, its
//! length is the key's own. Before the first key and after the last, the curve
//! goes on as its infinity says: Constant, Linear, Cycle, Cycle with Offset,
//! Oscillate.

use serde::{Deserialize, Serialize};

/// How a side of a key sets its slope.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tangent {
    /// Smooth, without overshooting the keys on either side: flat at a peak or
    /// a trough and at the ends.
    #[default]
    Auto,
    /// Smooth through the keys on either side (Catmull-Rom).
    Spline,
    /// Spline, but flat where the key on either side has the same value.
    Clamped,
    /// Straight to the key on that side.
    Linear,
    /// Level.
    Flat,
    /// Holds the value until the next key (out side only; in reads Flat).
    Stepped,
    /// Spline, flat at peaks, troughs and ends, never overshooting.
    Plateau,
    /// As dragged in the Curve Editor: `in_slope`, `out_slope`.
    Fixed,
}

impl Tangent {
    pub const ALL: [Tangent; 7] = [
        Tangent::Auto,
        Tangent::Spline,
        Tangent::Clamped,
        Tangent::Linear,
        Tangent::Flat,
        Tangent::Stepped,
        Tangent::Plateau,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Tangent::Auto => "Auto",
            Tangent::Spline => "Spline",
            Tangent::Clamped => "Clamped",
            Tangent::Linear => "Linear",
            Tangent::Flat => "Flat",
            Tangent::Stepped => "Stepped",
            Tangent::Plateau => "Plateau",
            Tangent::Fixed => "Fixed",
        }
    }
}

/// What a curve does before its first key and after its last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Infinity {
    /// The end key's value.
    #[default]
    Constant,
    /// On along the end key's tangent.
    Linear,
    /// The keys again and again.
    Cycle,
    /// Again and again, each time moved by the difference between the last key
    /// and the first, so a walk keeps walking.
    CycleOffset,
    /// Forwards, then backwards.
    Oscillate,
}

impl Infinity {
    pub const ALL: [Infinity; 5] = [
        Infinity::Constant,
        Infinity::Linear,
        Infinity::Cycle,
        Infinity::CycleOffset,
        Infinity::Oscillate,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Infinity::Constant => "Constant",
            Infinity::Linear => "Linear",
            Infinity::Cycle => "Cycle",
            Infinity::CycleOffset => "Cycle with Offset",
            Infinity::Oscillate => "Oscillate",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub frame: f64,
    pub value: f64,
    #[serde(default)]
    pub in_type: Tangent,
    #[serde(default)]
    pub out_type: Tangent,
    /// Value per frame, read when the side is Fixed.
    #[serde(default)]
    pub in_slope: f64,
    #[serde(default)]
    pub out_slope: f64,
    /// Handle length in frames, read when the channel is weighted. Zero: a
    /// third of the way to the key on that side.
    #[serde(default)]
    pub in_weight: f64,
    #[serde(default)]
    pub out_weight: f64,
    /// The in and out handles move apart.
    #[serde(default)]
    pub broken: bool,
}

impl Key {
    pub fn new(frame: f64, value: f64) -> Key {
        Key {
            frame,
            value,
            in_type: Tangent::Auto,
            out_type: Tangent::Auto,
            in_slope: 0.0,
            out_slope: 0.0,
            in_weight: 0.0,
            out_weight: 0.0,
            broken: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    /// Where among the operator's parameters: `translation/0`.
    pub path: String,
    /// In frame order.
    pub keys: Vec<Key>,
    #[serde(default)]
    pub pre: Infinity,
    #[serde(default)]
    pub post: Infinity,
    #[serde(default)]
    pub weighted: bool,
}

impl Channel {
    pub fn new(path: &str) -> Channel {
        Channel {
            path: path.to_string(),
            keys: vec![],
            pre: Infinity::Constant,
            post: Infinity::Constant,
            weighted: false,
        }
    }

    /// Keys in frame order, two never on the same frame.
    pub fn sort(&mut self) {
        self.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        self.keys.dedup_by(|b, a| (a.frame - b.frame).abs() < 1e-6);
    }

    /// The key on `frame`, if there is one.
    pub fn key_at(&self, frame: f64) -> Option<usize> {
        self.keys.iter().position(|k| (k.frame - frame).abs() < 1e-6)
    }

    /// Set a key at `frame` (replacing the value of one there).
    pub fn set(&mut self, frame: f64, value: f64) -> usize {
        match self.key_at(frame) {
            Some(i) => {
                self.keys[i].value = value;
                i
            }
            None => {
                self.keys.push(Key::new(frame, value));
                self.sort();
                self.key_at(frame).unwrap_or(0)
            }
        }
    }

    /// The slopes of key `i`, in and out, in value per frame.
    pub fn slopes(&self, i: usize) -> (f64, f64) {
        let k = &self.keys[i];
        let prev = i.checked_sub(1).map(|j| &self.keys[j]);
        let next = self.keys.get(i + 1);
        let secant = |a: &Key, b: &Key| {
            if (b.frame - a.frame).abs() < 1e-9 {
                0.0
            } else {
                (b.value - a.value) / (b.frame - a.frame)
            }
        };
        let s_prev = prev.map(|p| secant(p, k));
        let s_next = next.map(|n| secant(k, n));
        let spline = match (prev, next) {
            (Some(p), Some(n)) => secant(p, n),
            (Some(_), None) => s_prev.unwrap_or(0.0),
            (None, Some(_)) => s_next.unwrap_or(0.0),
            (None, None) => 0.0,
        };
        let extreme = match (prev, next) {
            (Some(p), Some(n)) => {
                (k.value >= p.value && k.value >= n.value) || (k.value <= p.value && k.value <= n.value)
            }
            _ => true,
        };
        let clamp3 = |s: f64| {
            // No more than three times the gentler secant: the curve stays
            // between its keys (the monotone rule).
            let lim = [s_prev, s_next]
                .iter()
                .flatten()
                .map(|x| x.abs() * 3.0)
                .fold(f64::MAX, f64::min);
            s.clamp(-lim, lim)
        };
        let of = |t: Tangent, inward: bool| -> f64 {
            match t {
                Tangent::Fixed => {
                    if inward {
                        k.in_slope
                    } else {
                        k.out_slope
                    }
                }
                Tangent::Flat | Tangent::Stepped => 0.0,
                Tangent::Linear => {
                    if inward {
                        s_prev.or(s_next).unwrap_or(0.0)
                    } else {
                        s_next.or(s_prev).unwrap_or(0.0)
                    }
                }
                Tangent::Spline => spline,
                Tangent::Clamped => {
                    let same =
                        |o: Option<&Key>| o.is_some_and(|o| (o.value - k.value).abs() <= 1e-9 * (1.0 + k.value.abs()));
                    if same(prev) || same(next) {
                        0.0
                    } else {
                        spline
                    }
                }
                Tangent::Plateau => {
                    if extreme {
                        0.0
                    } else {
                        clamp3(spline)
                    }
                }
                Tangent::Auto => match (s_prev, s_next) {
                    (Some(a), Some(b)) if a * b > 0.0 => {
                        // Weighted harmonic mean of the secants (Fritsch-Butland).
                        let (h0, h1) = (k.frame - prev.unwrap().frame, next.unwrap().frame - k.frame);
                        let (w1, w2) = (2.0 * h1 + h0, h1 + 2.0 * h0);
                        (w1 + w2) / (w1 / a + w2 / b)
                    }
                    _ => 0.0,
                },
            }
        };
        // An unbroken Fixed key has one slope through it: dragging a handle
        // sets both (`set_slope`).
        (of(k.in_type, true), of(k.out_type, false))
    }

    /// Drag a handle: the side becomes Fixed at `slope`; an unbroken key turns
    /// both sides together. `weight`: the handle's new length in frames, for a
    /// weighted channel.
    pub fn set_slope(&mut self, i: usize, inward: bool, slope: f64, weight: Option<f64>) {
        let (s_in, s_out) = self.slopes(i);
        let weighted = self.weighted;
        let k = &mut self.keys[i];
        let (mut si, mut so) = (s_in, s_out);
        if inward {
            si = slope
        } else {
            so = slope
        }
        if !k.broken {
            si = slope;
            so = slope;
        }
        k.in_type = Tangent::Fixed;
        k.out_type = Tangent::Fixed;
        k.in_slope = si;
        k.out_slope = so;
        if let (true, Some(w)) = (weighted, weight) {
            if inward {
                k.in_weight = w.max(0.01)
            } else {
                k.out_weight = w.max(0.01)
            }
        }
    }

    /// The handle lengths of the segment from key `i` to key `i + 1`, in
    /// frames.
    pub fn weights(&self, i: usize) -> (f64, f64) {
        let (a, b) = (&self.keys[i], &self.keys[i + 1]);
        let dt = b.frame - a.frame;
        if !self.weighted {
            return (dt / 3.0, dt / 3.0);
        }
        let w = |x: f64| if x > 0.0 { x.min(dt) } else { dt / 3.0 };
        (w(a.out_weight), w(b.in_weight))
    }

    /// The four Bezier points of the segment from key `i` to `i + 1`.
    pub fn segment(&self, i: usize) -> [(f64, f64); 4] {
        let (a, b) = (&self.keys[i], &self.keys[i + 1]);
        let (_, s0) = self.slopes(i);
        let (s1, _) = self.slopes(i + 1);
        let (w0, w1) = self.weights(i);
        [
            (a.frame, a.value),
            (a.frame + w0, a.value + s0 * w0),
            (b.frame - w1, b.value - s1 * w1),
            (b.frame, b.value),
        ]
    }

    /// The curve's value at `frame`.
    pub fn value_at(&self, frame: f64) -> f64 {
        let n = self.keys.len();
        if n == 0 {
            return 0.0;
        }
        if n == 1 {
            return self.keys[0].value;
        }
        let (first, last) = (self.keys[0], self.keys[n - 1]);
        let span = last.frame - first.frame;
        if frame < first.frame || frame > last.frame {
            let before = frame < first.frame;
            let inf = if before { self.pre } else { self.post };
            match inf {
                Infinity::Constant => return if before { first.value } else { last.value },
                Infinity::Linear => {
                    return if before {
                        first.value + self.slopes(0).0 * (frame - first.frame)
                    } else {
                        last.value + self.slopes(n - 1).1 * (frame - last.frame)
                    };
                }
                _ if span <= 1e-9 => return first.value,
                Infinity::Cycle | Infinity::CycleOffset => {
                    let cycles = ((frame - first.frame) / span).floor();
                    let local = frame - cycles * span;
                    let offset = if inf == Infinity::CycleOffset {
                        cycles * (last.value - first.value)
                    } else {
                        0.0
                    };
                    return self.inside(local) + offset;
                }
                Infinity::Oscillate => {
                    let cycles = ((frame - first.frame) / span).floor();
                    let mut local = frame - cycles * span;
                    if (cycles as i64).rem_euclid(2) == 1 {
                        local = first.frame + last.frame - local;
                    }
                    return self.inside(local);
                }
            }
        }
        self.inside(frame)
    }

    /// The value at a frame between the first key and the last.
    fn inside(&self, frame: f64) -> f64 {
        let n = self.keys.len();
        let i = match self.keys.iter().rposition(|k| k.frame <= frame) {
            Some(i) => i.min(n - 2),
            None => 0,
        };
        let (a, b) = (&self.keys[i], &self.keys[i + 1]);
        if frame >= b.frame {
            return b.value;
        }
        if frame <= a.frame || a.out_type == Tangent::Stepped {
            return a.value;
        }
        let p = self.segment(i);
        let bez = |u: f64, k: usize| {
            let v = 1.0 - u;
            let c = |q: (f64, f64)| if k == 0 { q.0 } else { q.1 };
            v * v * v * c(p[0]) + 3.0 * v * v * u * c(p[1]) + 3.0 * v * u * u * c(p[2]) + u * u * u * c(p[3])
        };
        // The handles never pass each other, so frame grows with u: halve.
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..48 {
            let mid = 0.5 * (lo + hi);
            if bez(mid, 0) < frame {
                lo = mid
            } else {
                hi = mid
            }
        }
        bez(0.5 * (lo + hi), 1)
    }
}

// Where the channels live among an operator's parameters

/// The numbers among an operator's parameters that can be keyed, by path, with
/// their values: fields, and vectors of up to four numbers. Lists of rows (Edit
/// Poly's operations, Set Attribute's rows) are not, nor switches and text.
/// `value` is the operator's parameters as saved.
pub fn keyable(value: &serde_json::Value) -> Vec<(String, f64)> {
    let mut out = vec![];
    // The operator's kind is the one key around its parameters.
    let inner = match value {
        serde_json::Value::Object(o) if o.len() == 1 => o.values().next().unwrap(),
        v => v,
    };
    walk(inner, String::new(), 0, &mut out);
    out
}

fn walk(v: &serde_json::Value, path: String, depth: usize, out: &mut Vec<(String, f64)>) {
    if depth > 4 {
        return;
    }
    let join = |p: &str, k: &str| {
        if p.is_empty() {
            k.to_string()
        } else {
            format!("{p}/{k}")
        }
    };
    match v {
        serde_json::Value::Number(n) => {
            if let Some(x) = n.as_f64() {
                out.push((path, x));
            }
        }
        serde_json::Value::Array(a) if a.len() <= 4 && a.iter().all(|x| x.is_number()) => {
            for (i, x) in a.iter().enumerate() {
                walk(x, join(&path, &i.to_string()), depth + 1, out);
            }
        }
        serde_json::Value::Object(o) => {
            for (k, x) in o {
                // What names the data rather than shapes it.
                if matches!(k.as_str(), "id" | "take" | "index" | "fps_num" | "fps_den" | "version") {
                    continue;
                }
                walk(x, join(&path, k), depth + 1, out);
            }
        }
        _ => {}
    }
}

/// The value at `path` among the parameters.
pub fn get(value: &serde_json::Value, path: &str) -> Option<f64> {
    let mut v = match value {
        serde_json::Value::Object(o) if o.len() == 1 => o.values().next()?,
        v => v,
    };
    for part in path.split('/') {
        v = match v {
            serde_json::Value::Object(o) => o.get(part)?,
            serde_json::Value::Array(a) => a.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    v.as_f64()
}

/// Set the value at `path`. A whole number stays whole (rounded).
pub fn set(value: &mut serde_json::Value, path: &str, x: f64) -> bool {
    let mut v = match value {
        serde_json::Value::Object(o) if o.len() == 1 => match o.values_mut().next() {
            Some(v) => v,
            None => return false,
        },
        v => v,
    };
    for part in path.split('/') {
        v = match v {
            serde_json::Value::Object(o) => match o.get_mut(part) {
                Some(v) => v,
                None => return false,
            },
            serde_json::Value::Array(a) => match part.parse::<usize>().ok().and_then(|i| a.get_mut(i)) {
                Some(v) => v,
                None => return false,
            },
            _ => return false,
        };
    }
    let serde_json::Value::Number(n) = v else { return false };
    *v = if n.is_u64() {
        serde_json::json!(x.round().max(0.0) as u64)
    } else if n.is_i64() {
        serde_json::json!(x.round() as i64)
    } else {
        serde_json::Number::from_f64(x)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::json!(0.0))
    };
    true
}

/// A path as people read it: `translation/0` is "Translation X".
pub fn label(path: &str) -> String {
    // A Local Animation's keys name the parameters it inherits.
    let path = path.strip_prefix("inherited/").unwrap_or(path);
    let parts: Vec<String> = path
        .split('/')
        .map(|p| match p {
            "0" => "X".into(),
            "1" => "Y".into(),
            "2" => "Z".into(),
            "3" => "W".into(),
            w => {
                let w = w.replace('_', " ");
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_uppercase().chain(c).collect())
                    .unwrap_or_default()
            }
        })
        .collect();
    parts.join(" ")
}

/// How a channel's numbers are shown: a Transform's rotation is kept in radians
/// and shown in degrees, as its parameters show it.
pub fn display_scale(kind: &str, path: &str) -> f64 {
    if kind == "Transform" && path.starts_with("rotation/") {
        180.0 / std::f64::consts::PI
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(keys: &[(f64, f64)]) -> Channel {
        let mut c = Channel::new("x");
        for (f, v) in keys {
            c.set(*f, *v);
        }
        c
    }

    /// The curve passes through its keys; linear tangents make straight lines;
    /// stepped holds; flat is level at the keys.
    #[test]
    fn curves_pass_through_their_keys() {
        let mut c = ch(&[(1.0, 0.0), (11.0, 10.0), (21.0, 0.0)]);
        for k in c.keys.clone() {
            assert!((c.value_at(k.frame) - k.value).abs() < 1e-9);
        }
        for k in &mut c.keys {
            k.in_type = Tangent::Linear;
            k.out_type = Tangent::Linear;
        }
        assert!((c.value_at(6.0) - 5.0).abs() < 1e-6);
        assert!((c.value_at(16.0) - 5.0).abs() < 1e-6);
        c.keys[0].out_type = Tangent::Stepped;
        assert_eq!(c.value_at(10.9), 0.0);
        assert_eq!(c.value_at(11.0), 10.0);
        // Auto: flat at the peak, no overshoot past it.
        let c = ch(&[(1.0, 0.0), (11.0, 10.0), (21.0, 0.0)]);
        assert_eq!(c.slopes(1), (0.0, 0.0));
        assert!((1..=21).all(|f| c.value_at(f as f64) <= 10.0 + 1e-9));
        // Spline overshoots between rising keys of unequal steps; Auto does not.
        let mut s = ch(&[(0.0, 0.0), (1.0, 10.0), (10.0, 11.0)]);
        for k in &mut s.keys {
            k.in_type = Tangent::Spline;
            k.out_type = Tangent::Spline;
        }
        let a = ch(&[(0.0, 0.0), (1.0, 10.0), (10.0, 11.0)]);
        let max = |c: &Channel| (0..=100).map(|i| c.value_at(i as f64 * 0.1)).fold(f64::MIN, f64::max);
        assert!(max(&s) > 11.0 + 1e-3, "spline overshoots: {}", max(&s));
        assert!(max(&a) <= 11.0 + 1e-9, "auto does not: {}", max(&a));
    }

    /// Infinity: cycle repeats, cycle with offset keeps climbing, oscillate
    /// goes back, linear follows the end tangent.
    #[test]
    fn infinity_continues_the_curve() {
        let mut c = ch(&[(0.0, 0.0), (10.0, 4.0)]);
        for k in &mut c.keys {
            k.in_type = Tangent::Linear;
            k.out_type = Tangent::Linear;
        }
        assert_eq!(c.value_at(25.0), 4.0);
        c.post = Infinity::Linear;
        assert!((c.value_at(25.0) - 10.0).abs() < 1e-6);
        c.post = Infinity::Cycle;
        assert!((c.value_at(25.0) - 2.0).abs() < 1e-6);
        c.post = Infinity::CycleOffset;
        assert!((c.value_at(25.0) - 10.0).abs() < 1e-6);
        c.post = Infinity::Oscillate;
        assert!((c.value_at(15.0) - 2.0).abs() < 1e-6);
        c.pre = Infinity::Cycle;
        assert!((c.value_at(-5.0) - 2.0).abs() < 1e-6);
    }

    /// A weighted handle pulled out holds the value longer near its key.
    #[test]
    fn weighted_handles_shape_the_curve() {
        let mut c = ch(&[(0.0, 0.0), (10.0, 10.0)]);
        for k in &mut c.keys {
            k.in_type = Tangent::Flat;
            k.out_type = Tangent::Flat;
        }
        let plain = c.value_at(2.0);
        c.weighted = true;
        c.keys[0].out_weight = 9.0;
        assert!(c.value_at(2.0) < plain, "{} {}", c.value_at(2.0), plain);
        assert!((c.value_at(10.0) - 10.0).abs() < 1e-9);
    }

    /// The keyable numbers of an operator, found by path, read and written.
    #[test]
    fn parameters_are_found_by_path() {
        let t = crate::types::NodeType::Transform {
            translation: glam::Vec3::new(1.0, 2.0, 3.0),
            rotation: glam::Vec3::ZERO,
            scale: glam::Vec3::ONE,
        };
        let mut v = serde_json::to_value(&t).unwrap();
        let paths: Vec<String> = keyable(&v).into_iter().map(|(p, _)| p).collect();
        assert!(
            paths.contains(&"translation/1".to_string()) && paths.contains(&"scale/2".to_string()),
            "{paths:?}"
        );
        assert_eq!(get(&v, "translation/2"), Some(3.0));
        assert!(set(&mut v, "translation/2", 7.5));
        let back: crate::types::NodeType = serde_json::from_value(v).unwrap();
        assert!(matches!(back, crate::types::NodeType::Transform { translation, .. } if translation.z == 7.5));
        // A whole number stays whole.
        let s = crate::types::NodeType::CreateSphere {
            radius: 1.0,
            segments: 16,
            location: "/s".into(),
        };
        let mut v = serde_json::to_value(&s).unwrap();
        set(&mut v, "segments", 20.6);
        assert!(matches!(
            serde_json::from_value(v).unwrap(),
            crate::types::NodeType::CreateSphere { segments: 21, .. }
        ));
        assert_eq!(label("translation/0"), "Translation X");
    }
}
