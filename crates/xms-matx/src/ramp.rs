//! MaterialX's `ramp` node (1.39) as keys: its `num_intervals` keys, each an
//! `intervalN` (where, 0 to 1) and a `colorN` (RGBA), up to ten, and its
//! `interpolation` (0 linear, 1 smooth, 2 step). What Imago's ramp widget
//! edits in place of the forty-odd inputs.
//!
//! Arnold's `ramp_rgb` and `ramp_float` (`ai_`, from the SDK's definitions) as
//! keys too: their `position`, `color` (three numbers a key) or `value`, and
//! `interpolation` arrays, one interpolation a key (0 constant, 1 linear, 2
//! Catmull-Rom, 3 monotone cubic).

use crate::graph::{Binding, Graph};
use crate::library::Library;
use crate::types::Value;

/// The most keys a `ramp` takes.
pub const MAX_KEYS: usize = 10;
/// The fewest.
pub const MIN_KEYS: usize = 2;

/// A `ramp`'s keys (where, colour) and interpolation (0 linear, 1 smooth, 2
/// step).
#[derive(Clone, Debug, PartialEq)]
pub struct Keys {
    pub keys: Vec<(f32, [f32; 4])>,
    pub interpolation: i32,
}

/// Whether an input is one the keys stand for.
pub fn is_key_input(name: &str) -> bool {
    let numbered = |prefix: &str| {
        name.strip_prefix(prefix)
            .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    name == "num_intervals" || name == "interpolation" || numbered("interval") || numbered("color")
}

/// A `ramp` node's keys, as its values say. None for another node, or one
/// with an input of its keys connected (edited input by input then).
pub fn read(g: &Graph, lib: &Library, id: u32) -> Option<Keys> {
    let node = g.node(id)?;
    if node.node != "ramp" {
        return None;
    }
    if node
        .inputs
        .iter()
        .any(|(n, b)| is_key_input(n) && matches!(b, Binding::Connect { .. }))
    {
        return None;
    }
    let int = |name: &str| match g.value(lib, id, name) {
        Some(Value::Integer(i)) => Some(*i),
        _ => None,
    };
    let num = int("num_intervals")
        .unwrap_or(2)
        .clamp(MIN_KEYS as i32, MAX_KEYS as i32) as usize;
    let mut keys = vec![];
    for k in 1..=num {
        let at = match g.value(lib, id, &format!("interval{k}")) {
            Some(Value::Floats(f)) if !f.is_empty() => f[0],
            _ => 1.0,
        };
        let colour = match g.value(lib, id, &format!("color{k}")) {
            Some(Value::Floats(f)) if f.len() == 4 => [f[0], f[1], f[2], f[3]],
            _ => [1.0; 4],
        };
        keys.push((at, colour));
    }
    Some(Keys {
        keys,
        interpolation: int("interpolation").unwrap_or(1),
    })
}

/// Keys put on a `ramp` node: in order, at most ten (the first ten by place),
/// two at least (the last repeated); the inputs of keys it no longer has back
/// to their defaults.
pub fn write(g: &mut Graph, id: u32, keys: &Keys) {
    let mut k = keys.keys.clone();
    k.sort_by(|a, b| a.0.total_cmp(&b.0));
    k.truncate(MAX_KEYS);
    while k.len() < MIN_KEYS {
        let last = k.last().copied().unwrap_or((1.0, [1.0; 4]));
        k.push(last);
    }
    g.set_value(id, "num_intervals", Value::Integer(k.len() as i32));
    g.set_value(id, "interpolation", Value::Integer(keys.interpolation.clamp(0, 2)));
    for (n, (at, c)) in k.iter().enumerate() {
        g.set_value(
            id,
            &format!("interval{}", n + 1),
            Value::Floats(vec![at.clamp(0.0, 1.0)]),
        );
        g.set_value(id, &format!("color{}", n + 1), Value::Floats(c.to_vec()));
    }
    for n in k.len() + 1..=MAX_KEYS {
        g.reset(id, &format!("interval{n}"));
        g.reset(id, &format!("color{n}"));
    }
}

/// An Arnold ramp's keys: whether a colour (`ramp_rgb`) or a float
/// (`ramp_float`), its keys (a float's value in the colour's first number,
/// the others the same: a grey), and their one interpolation (None: the keys
/// differ).
#[derive(Clone, Debug, PartialEq)]
pub struct ArnoldKeys {
    pub colour: bool,
    pub keys: Vec<(f32, [f32; 4])>,
    pub interpolation: Option<i32>,
}

/// Whether a node is an Arnold ramp: a colour one, a float one.
pub fn arnold_ramp(lib: &Library, node: &str) -> Option<bool> {
    let (r, own) = lib.renderer_of(node)?;
    if r.context != "arnold" {
        return None;
    }
    match own {
        "ramp_rgb" => Some(true),
        "ramp_float" => Some(false),
        _ => None,
    }
}

/// The inputs an Arnold ramp's keys stand for.
pub fn is_arnold_key_input(name: &str) -> bool {
    matches!(name, "position" | "color" | "value" | "interpolation")
}

/// An Arnold ramp's keys, as its arrays say. None for another node, or one
/// with an input of its keys connected.
pub fn read_arnold(g: &Graph, lib: &Library, id: u32) -> Option<ArnoldKeys> {
    let node = g.node(id)?;
    let colour = arnold_ramp(lib, &node.node)?;
    if node
        .inputs
        .iter()
        .any(|(n, b)| is_arnold_key_input(n) && matches!(b, Binding::Connect { .. }))
    {
        return None;
    }
    let floats = |name: &str| match g.value(lib, id, name) {
        Some(Value::FloatArray(f)) | Some(Value::Floats(f)) => f.clone(),
        _ => vec![],
    };
    let position = floats("position");
    let values = floats(if colour { "color" } else { "value" });
    let interps = match g.value(lib, id, "interpolation") {
        Some(Value::IntegerArray(v)) => v.clone(),
        _ => vec![],
    };
    let keys: Vec<(f32, [f32; 4])> = position
        .iter()
        .enumerate()
        .filter_map(|(k, &at)| {
            if colour {
                let c = values.get(k * 3..k * 3 + 3)?;
                Some((at, [c[0], c[1], c[2], 1.0]))
            } else {
                let v = *values.get(k)?;
                Some((at, [v, v, v, 1.0]))
            }
        })
        .collect();
    let first = interps.first().copied().unwrap_or(1);
    let same = interps.iter().take(keys.len()).all(|i| *i == first);
    Some(ArnoldKeys {
        colour,
        keys,
        interpolation: same.then_some(first),
    })
}

/// Keys put on an Arnold ramp, in order: its `position`, `color` or `value`,
/// and `interpolation` (each key `interpolation`; None: as they were, by their
/// place in order, the last's for a key added).
pub fn write_arnold(g: &mut Graph, lib: &Library, id: u32, keys: &ArnoldKeys) {
    let mut k = keys.keys.clone();
    k.sort_by(|a, b| a.0.total_cmp(&b.0));
    let position: Vec<f32> = k.iter().map(|(at, _)| at.clamp(0.0, 1.0)).collect();
    let interps: Vec<i32> = match keys.interpolation {
        Some(i) => vec![i; k.len()],
        None => {
            let was = match g.value(lib, id, "interpolation") {
                Some(Value::IntegerArray(v)) => v.clone(),
                _ => vec![],
            };
            let last = was.last().copied().unwrap_or(1);
            (0..k.len()).map(|n| was.get(n).copied().unwrap_or(last)).collect()
        }
    };
    if keys.colour {
        let colours: Vec<f32> = k.iter().flat_map(|(_, c)| [c[0], c[1], c[2]]).collect();
        g.set_value(id, "color", Value::FloatArray(colours));
    } else {
        g.set_value(id, "value", Value::FloatArray(k.iter().map(|(_, c)| c[0]).collect()));
    }
    g.set_value(id, "position", Value::FloatArray(position));
    g.set_value(id, "interpolation", Value::IntegerArray(interps));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ramp_s_keys_are_read_and_written() {
        let lib = Library::standard();
        let mut g = Graph::new();
        let id = g.add(lib, "ramp", [0.0, 0.0]).unwrap();
        // A new ramp: black to white, smooth.
        let k = read(&g, lib, id).unwrap();
        assert_eq!(k.keys, [(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0; 4])]);
        assert_eq!(k.interpolation, 1);
        // Three keys out of order, step: written in order.
        let keys = Keys {
            keys: vec![
                (1.0, [0.0, 0.0, 1.0, 1.0]),
                (0.0, [1.0, 0.0, 0.0, 1.0]),
                (0.4, [0.0, 1.0, 0.0, 1.0]),
            ],
            interpolation: 2,
        };
        write(&mut g, id, &keys);
        let back = read(&g, lib, id).unwrap();
        assert_eq!(back.keys.len(), 3);
        assert_eq!(back.keys[1], (0.4, [0.0, 1.0, 0.0, 1.0]));
        assert_eq!(back.interpolation, 2);
        // Back to two: the third's inputs back to their defaults.
        write(
            &mut g,
            id,
            &Keys {
                keys: back.keys[..2].to_vec(),
                interpolation: 0,
            },
        );
        assert!(g.node(id).unwrap().binding("interval3").is_none());
        assert_eq!(read(&g, lib, id).unwrap().keys.len(), 2);
        // One of its keys connected: not read as keys.
        let c = g.add(lib, "constant", [0.0, 0.0]);
        if let Some(c) = c {
            if g.connect(lib, c, "out", id, "color1").is_ok() {
                assert!(read(&g, lib, id).is_none());
            }
        }
        assert!(is_key_input("color10") && is_key_input("num_intervals") && !is_key_input("texcoord"));
        assert!(!is_key_input("color"));
    }

    /// Arnold's ramps as the SDK defines them (Arnold 7.5's nodedefs).
    fn with_arnold_ramps() -> Library {
        let arnold = crate::Renderer {
            prefix: "ai_".into(),
            context: "arnold".into(),
            label: "Arnold".into(),
        };
        let text = r#"<?xml version="1.0"?>
<materialx version="1.38">
  <nodedef name="ARNOLD_ND_ramp_float" node="ramp_float">
    <output name="out" type="float" />
    <input name="type" type="integer" value="0" enum="custom,u,v" enumvalues="0,1,2" />
    <input name="input" type="float" value="0" />
    <input name="position" type="floatarray" value="0.0, 1.0" />
    <input name="value" type="floatarray" value="0.0, 1.0" />
    <input name="interpolation" type="integerarray" value="2, 2" />
  </nodedef>
  <nodedef name="ARNOLD_ND_ramp_rgb" node="ramp_rgb">
    <output name="out" type="color3" />
    <input name="type" type="integer" value="0" enum="custom,u,v" enumvalues="0,1,2" />
    <input name="input" type="float" value="0" />
    <input name="position" type="floatarray" value="0.0, 1.0" />
    <input name="color" type="floatarray" value="0.0, 0.0, 0.0, 1.0, 1.0, 1.0" />
    <input name="interpolation" type="integerarray" value="2, 2" />
  </nodedef>
</materialx>"#;
        let (lib, notes) = Library::standard()
            .with_renderer(&arnold, "nodedefs.mtlx", text)
            .unwrap();
        assert!(notes.is_empty(), "{notes:?}");
        lib
    }

    #[test]
    fn an_arnold_ramp_s_arrays_are_its_keys() {
        let lib = with_arnold_ramps();
        let mut g = Graph::new();
        let rgb = g.add(&lib, "ai_ramp_rgb", [0.0, 0.0]).unwrap();
        let k = read_arnold(&g, &lib, rgb).unwrap();
        assert!(k.colour);
        assert_eq!(k.keys, [(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0; 4])]);
        assert_eq!(k.interpolation, Some(2));
        // Three keys, linear: the colours three numbers a key.
        let keys = ArnoldKeys {
            colour: true,
            keys: vec![
                (1.0, [0.0, 0.0, 1.0, 1.0]),
                (0.0, [1.0, 0.0, 0.0, 1.0]),
                (0.5, [0.0, 1.0, 0.0, 1.0]),
            ],
            interpolation: Some(1),
        };
        write_arnold(&mut g, &lib, rgb, &keys);
        assert_eq!(
            g.value(&lib, rgb, "position"),
            Some(&Value::FloatArray(vec![0.0, 0.5, 1.0]))
        );
        assert_eq!(
            g.value(&lib, rgb, "color"),
            Some(&Value::FloatArray(vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]))
        );
        assert_eq!(
            g.value(&lib, rgb, "interpolation"),
            Some(&Value::IntegerArray(vec![1, 1, 1]))
        );
        // A float ramp's keys differing in interpolation: kept, a key added the last's.
        let f = g.add(&lib, "ai_ramp_float", [0.0, 0.0]).unwrap();
        g.set_value(f, "interpolation", Value::IntegerArray(vec![0, 3]));
        let mut k = read_arnold(&g, &lib, f).unwrap();
        assert!(!k.colour && k.interpolation.is_none());
        k.keys.push((0.5, [0.25, 0.25, 0.25, 1.0]));
        write_arnold(&mut g, &lib, f, &k);
        assert_eq!(
            g.value(&lib, f, "value"),
            Some(&Value::FloatArray(vec![0.0, 0.25, 1.0]))
        );
        assert_eq!(
            g.value(&lib, f, "interpolation"),
            Some(&Value::IntegerArray(vec![0, 3, 3]))
        );
        assert!(arnold_ramp(&lib, "ai_standard_surface").is_none());
        assert!(read_arnold(&g, &lib, rgb).is_some() && is_arnold_key_input("color"));
    }
}
