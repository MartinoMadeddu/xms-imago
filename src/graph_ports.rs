//! What every graph of XMS does the same way with its ports, wires and
//! selection (the scene graph, ICE graphs, MaterialX graphs):
//!
//! - **Unplugging by clicking:** click a port that has a wire (an input, or
//!   an output with one or more), then click the empty canvas: its wires
//!   come off. The picked port shows a ring and its wires the selection
//!   colour until then; Escape, or a click anywhere else, lets it go. An
//!   output picked this way still draws a new wire, so clicking an input
//!   instead connects it as before.
//! - **Unplugging in one motion:** drag from an input with a wire: the wire
//!   comes off and follows the pointer, into another input or, let go of
//!   over the canvas, nowhere. Ctrl+drag (Cmd on macOS) from an output
//!   picks up all its wires, to another output or nowhere. A plain drag from
//!   an output draws a new wire: an output feeds any number of inputs.
//! - **Box select:** drag on the empty canvas to select every node the box
//!   touches; dragging one of them moves them all; Delete deletes them.
//! - **Wires:** click a wire to select it; Delete takes it off.
//!
//! Each graph keeps its own state (its nodes and ports are its own types);
//! this module holds the shared shape of it and its drawing.

/// A port picked by a click, to be unplugged by a click on the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked<N, P> {
    /// An input and the wire into it.
    Input(N, P),
    /// An output and every wire out of it.
    Output(N, P),
}

impl<N: Copy + PartialEq, P: PartialEq> Picked<N, P> {
    /// The node the port is on.
    pub fn node(&self) -> N {
        match self { Picked::Input(n, _) | Picked::Output(n, _) => *n }
    }

    /// Whether a wire from `from`'s output `out` into `to`'s input `inp`
    /// is one of the picked port's.
    pub fn holds(&self, from: N, out: &P, to: N, inp: &P) -> bool {
        match self {
            Picked::Input(n, p) => *n == to && p == inp,
            Picked::Output(n, p) => *n == from && p == out,
        }
    }
}

/// The ring around a picked port: in the selection colour, a little wider
/// than the port.
pub fn draw_picked(painter: &egui::Painter, at: egui::Pos2, radius: f32) {
    painter.circle_stroke(at, radius + 3.0, egui::Stroke::new(2.0, crate::theme::selection()));
}

/// The box being dragged out to select nodes, in screen space. `light`:
/// over a light canvas (ICE, MaterialX), where the dark schemes' tint would
/// not show.
pub fn draw_marquee(painter: &egui::Painter, r: egui::Rect, light: bool) {
    let (fill, edge) = if light || !crate::theme::is_dark() {
        crate::theme::marquee_light()
    } else {
        (egui::Color32::from_rgba_premultiplied(100, 140, 200, 40), crate::theme::c(120, 160, 220))
    };
    painter.rect_filled(r, 2.0, fill);
    painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0, edge), egui::StrokeKind::Middle);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picked_port_holds_its_wires() {
        let input: Picked<u32, usize> = Picked::Input(2, 0);
        assert!(input.holds(1, &0, 2, &0));
        assert!(!input.holds(1, &0, 2, &1), "another input of the node");
        let output: Picked<u32, usize> = Picked::Output(1, 0);
        assert!(output.holds(1, &0, 2, &0) && output.holds(1, &0, 3, &4), "every wire out of it");
        assert!(!output.holds(1, &1, 2, &0));
        assert_eq!(output.node(), 1);
    }
}
