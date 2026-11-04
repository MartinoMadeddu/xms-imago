//! Timeline panel.
//!
//! The timeline has no range, rate or timecode of its own. Every frame it takes
//! them from the clip produced by the selected node, so selecting a Trim node
//! shows the trimmed range and selecting a Retime node shows the new rate. The
//! clip entering the node is drawn behind it for context.

pub mod ui;

use std::sync::Arc;

use crate::core::anim::AnimData;
use crate::node_graph::NodeGraphState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RulerUnit {
    Frames,
    Timecode,
}

pub struct Playback {
    /// Playhead on the timecode axis, in seconds (see `frame_to_tc_seconds`).
    /// Not a frame number, so it stays on the same timecode when the selection
    /// changes rate.
    pub time: f64,
    /// Fraction of a frame accumulated while playing.
    pub frac: f64,
    pub playing: bool,
    pub looping: bool,
    pub ruler: RulerUnit,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            time: 0.0,
            frac: 0.0,
            playing: false,
            looping: true,
            ruler: RulerUnit::Frames,
        }
    }
}

/// What the timeline is showing this frame.
pub struct TimelineSource {
    pub node_name: String,
    /// Clip produced by the node. Defines range, rate and timecode.
    pub clip: Arc<AnimData>,
    /// Clip entering the node, if any.
    pub input: Option<Arc<AnimData>>,
    /// False when nothing is selected and the viewed node is used instead.
    pub from_selection: bool,
}

/// Why there is no source, for the empty-state label.
pub enum TimelineState {
    Source(TimelineSource),
    /// The selected node gives no clip: the scene range is shown.
    NoTimeData,
    Nothing,
}

/// Resolve the timeline source: the selected node, else the viewed node.
pub fn resolve_source(graph: &NodeGraphState) -> TimelineState {
    let (id, from_selection) = match graph.selected_node {
        Some(id) => (id, true),
        None => match graph.display_source() {
            Some(id) => (id, false),
            None => return TimelineState::Nothing,
        },
    };
    let Some(node) = graph.nodes.iter().find(|n| n.id == id) else {
        return TimelineState::Nothing;
    };
    let Some(clip) = graph.eval_anim(id) else {
        return if from_selection {
            TimelineState::NoTimeData
        } else {
            TimelineState::Nothing
        };
    };
    let input = if node.node_type.is_anim_generator() {
        None
    } else {
        node.inputs
            .first()
            .and_then(|s| s.connected_output)
            .and_then(|(src, out)| graph.eval_anim_out(src, out))
            // An operator that passes the clip on unchanged has no input to compare.
            .filter(|i| !Arc::ptr_eq(i, &clip))
    };
    TimelineState::Source(TimelineSource {
        node_name: node.name.clone(),
        clip,
        input,
        from_selection,
    })
}

/// The frames the timeline plays, as playhead times, and their rate (numerator
/// and denominator): the clip's, else the scene's range. A clip of one frame
/// is a pose: the scene's range.
pub fn frames(graph: &NodeGraphState) -> (Vec<f64>, (u32, u32)) {
    match resolve_source(graph) {
        TimelineState::Source(s) if s.clip.end_frame() > s.clip.start_frame => {
            let c = &s.clip;
            (
                (c.start_frame..=c.end_frame()).map(|f| c.tc_seconds(f)).collect(),
                (c.rate.num, c.rate.den),
            )
        }
        _ => {
            let sc = graph.scene;
            let end = sc.end.max(sc.start + 1);
            (
                (sc.start..=end).map(|f| sc.seconds(f as f64)).collect(),
                (sc.fps_num, sc.fps_den),
            )
        }
    }
}
