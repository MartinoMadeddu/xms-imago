//! MaterialX's `ramp` node (1.39) as keys: its `num_intervals` keys, each an
//! `intervalN` (where, 0 to 1) and a `colorN` (RGBA), up to ten, and its
//! `interpolation` (0 linear, 1 smooth, 2 step). What Imago's ramp widget
//! edits in place of the forty-odd inputs.

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
}
