//! The rig operators cooked: each input by its port (an input left empty is
//! None, not the next one), and what they found kept for their panels.

use std::sync::{Arc, Mutex};

use super::guides::{add_part, GuideEdit};
use super::mush::BakeReport;
use super::rom::RomReport;
use super::surface::Surface;
use super::{fit, Component, Finger, PartInfo, Side};
use crate::core::anim::AnimData;
use crate::types::{EvalResult, NodeType, Prim};

/// The primitives on a port: a scene's, or one mesh.
fn prims(r: Option<&EvalResult>) -> Option<Vec<Prim>> {
    match r? {
        EvalResult::Named(p) => Some(p.clone()),
        EvalResult::Single(m) => Some(vec![Prim::new("/mesh".into(), m.clone())]),
        _ => None,
    }
}

fn clip(r: Option<&EvalResult>) -> Option<&Arc<AnimData>> {
    r?.as_anim()
}

/// A Rig Guide's component, from its parameters. None for another operator.
pub fn part_of(t: &NodeType) -> Option<PartInfo> {
    let NodeType::RigGuide {
        component,
        side,
        index,
        attach,
        segments,
        twists,
        fingers,
        metacarpals,
        label,
        mech,
        ..
    } = t
    else {
        return None;
    };
    let component = *component;
    let side = if component.centred() {
        Side::Centre
    } else if component.sided() && *side == Side::Centre {
        Side::Left
    } else {
        *side
    };
    Some(PartInfo {
        component,
        side,
        index: *index,
        attach: (!attach.trim().is_empty()).then(|| attach.trim().to_string()),
        segments: *segments,
        twists: *twists,
        fingers: fingers.clone(),
        metacarpals: *metacarpals,
        label: label.clone(),
        mech: mech.clone(),
    })
}

/// What a body measured as, kept by its primitives: the components of one
/// guide chain all fit to the same mesh.
fn measured(prims: &[Prim]) -> (Vec<Surface>, Option<fit::Body>) {
    type Kept = (Vec<usize>, Vec<Surface>, Option<fit::Body>);
    static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
    let key: Vec<usize> = prims.iter().map(|p| Arc::as_ptr(&p.mesh) as usize).collect();
    if let Ok(k) = KEPT.lock() {
        if let Some((ks, s, b)) = k.as_ref() {
            if *ks == key {
                return (s.clone(), b.clone());
            }
        }
    }
    let surfaces: Vec<Surface> = prims.iter().filter_map(Surface::of).collect();
    let all: Vec<glam::Vec3> = surfaces.iter().flat_map(|s| s.points.iter().copied()).collect();
    let body = fit::measure(&all);
    if let Ok(mut k) = KEPT.lock() {
        *k = Some((key, surfaces.clone(), body.clone()));
    }
    (surfaces, body)
}

static ROM_REPORTS: Mutex<Vec<(usize, RomReport)>> = Mutex::new(vec![]);
static BAKE_REPORTS: Mutex<Vec<(usize, BakeReport)>> = Mutex::new(vec![]);

fn keep<T>(list: &Mutex<Vec<(usize, T)>>, key: usize, value: T) {
    if let Ok(mut l) = list.lock() {
        l.retain(|x| x.0 != key);
        l.push((key, value));
        if l.len() > 32 {
            l.remove(0);
        }
    }
}

/// What a Range of Motion's output holds.
pub fn rom_report(out: &Arc<AnimData>) -> Option<RomReport> {
    let key = Arc::as_ptr(out) as usize;
    ROM_REPORTS
        .lock()
        .ok()?
        .iter()
        .find(|x| x.0 == key)
        .map(|x| x.1.clone())
}

/// How a Bake Weights' output fits.
pub fn bake_report(out: &[Prim]) -> Option<BakeReport> {
    let key = out.first().map(|p| Arc::as_ptr(&p.mesh) as usize)?;
    BAKE_REPORTS
        .lock()
        .ok()?
        .iter()
        .find(|x| x.0 == key)
        .map(|x| x.1.clone())
}

/// A Rig Guide for a component, as the add menu and the templates make it.
pub fn guide(component: Component, side: Side) -> NodeType {
    NodeType::RigGuide {
        component,
        side: if component.centred() {
            Side::Centre
        } else if component.sided() && side == Side::Centre {
            Side::Left
        } else {
            side
        },
        index: 0,
        attach: String::new(),
        autofit: true,
        segments: match component {
            Component::Spine => 5,
            Component::Neck => 2,
            _ => 1,
        },
        twists: 2,
        fingers: Finger::ALL.to_vec(),
        metacarpals: true,
        edits: vec![],
        label: String::new(),
        mech: super::MechParams::of(component),
    }
}

pub fn is_rig(t: &NodeType) -> bool {
    matches!(
        t,
        NodeType::RigGuide { .. }
            | NodeType::BuildSkeleton { .. }
            | NodeType::HardBind
            | NodeType::Envelope { .. }
            | NodeType::RangeOfMotion { .. }
            | NodeType::DeltaMush { .. }
            | NodeType::BakeWeights { .. }
            | NodeType::RigidBind { .. }
            | NodeType::RigidSkin
            | NodeType::RigProxy
    )
}

/// Cook a rig operator. `time`: the playhead, seconds (Delta Mush's frame).
pub fn cook(t: &NodeType, ports: &[Option<EvalResult>], time: f64) -> Option<EvalResult> {
    let port = |k: usize| ports.get(k).and_then(|p| p.as_ref());
    match t {
        NodeType::RigGuide { autofit, edits, .. } => {
            let part = part_of(t)?;
            let mesh = prims(port(0)).filter(|p| !p.is_empty());
            let (surfaces, body) = match &mesh {
                Some(p) => measured(p),
                None => (vec![], None),
            };
            let bounds: Vec<(String, glam::Vec3, glam::Vec3)> = mesh
                .iter()
                .flatten()
                .filter_map(|p| crate::viewport::bounds::world_bounds(p).map(|b| (p.path.clone(), b.0, b.1)))
                .collect();
            let guides = add_part(
                clip(port(1)).map(|a| &**a),
                &part,
                (!surfaces.is_empty()).then_some(surfaces.as_slice()),
                body.as_ref().filter(|_| part.component.human()),
                &bounds,
                *autofit,
                edits,
            );
            // Without a mesh of its own, the one the guides came with.
            let guides = match (&guides.skin, clip(port(1)).and_then(|g| g.skin.clone())) {
                (None, Some(sk)) => {
                    let mut g = guides;
                    let mut sk = (*sk).clone();
                    sk.bind = vec![
                        g.joints.first().map(|j| j.rest.compute_matrix()).unwrap_or_default();
                        g.joints.len().max(1)
                    ];
                    g.skin = Some(Arc::new(sk));
                    g
                }
                _ => guides,
            };
            Some(EvalResult::Anim(Arc::new(guides)))
        }
        NodeType::BuildSkeleton { settings } => {
            let g = clip(port(0))?;
            super::build::build(g, settings)
                .ok()
                .map(|s| EvalResult::Anim(Arc::new(s)))
        }
        NodeType::HardBind => {
            let (p, sk) = (prims(port(0))?, clip(port(1))?);
            Some(EvalResult::Named(super::bind::hard_bind(&p, sk)))
        }
        NodeType::Envelope { edits } => {
            let (p, sk) = (prims(port(0))?, clip(port(1))?);
            Some(EvalResult::Named(super::bind::envelope(&p, sk, edits)))
        }
        NodeType::RangeOfMotion { settings } => {
            let sk = clip(port(0))?;
            let (rom, report) = super::rom::range_of_motion(sk, settings)?;
            let out = Arc::new(rom);
            keep(&ROM_REPORTS, Arc::as_ptr(&out) as usize, report);
            Some(EvalResult::Anim(out))
        }
        NodeType::DeltaMush { settings } => {
            let (p, rest, c) = (prims(port(0))?, clip(port(1))?, clip(port(2))?);
            let frame = c.index_at(time);
            Some(EvalResult::Named(super::mush::delta_mush(
                &p, rest, c, frame, *settings,
            )))
        }
        NodeType::BakeWeights { settings, samples } => {
            let (p, rest, c) = (prims(port(0))?, clip(port(1))?, clip(port(2))?);
            let (out, report) = super::mush::bake(&p, rest, c, *settings, (*samples).max(1) as usize);
            if let Some(first) = out.first() {
                keep(&BAKE_REPORTS, Arc::as_ptr(&first.mesh) as usize, report);
            }
            Some(EvalResult::Named(out))
        }
        NodeType::RigidBind { rows } => {
            let (p, sk) = (prims(port(0))?, clip(port(1))?);
            Some(EvalResult::Named(super::rigid::rigid_bind(&p, sk, rows)))
        }
        NodeType::RigidSkin => {
            let (p, rest, c) = (prims(port(0))?, clip(port(1))?, clip(port(2))?);
            Some(EvalResult::Named(super::rigid::rigid_skin(
                &p,
                rest,
                c,
                c.index_at(time),
            )))
        }
        NodeType::RigProxy => {
            let sk = clip(port(0))?;
            Some(EvalResult::Named(super::rigid::rig_proxy(sk)))
        }
        _ => None,
    }
}

/// The joints of a skeleton that deform, by name, for the panels' lists.
pub fn deforming(sk: &AnimData) -> Vec<String> {
    match sk.rig.as_deref().filter(|r| r.joints.len() == sk.joints.len()) {
        Some(r) => sk
            .joints
            .iter()
            .zip(&r.joints)
            .filter(|(_, i)| i.deform)
            .map(|(j, _)| j.name.clone())
            .collect(),
        None => sk.joints.iter().map(|j| j.name.clone()).collect(),
    }
}

/// A Rig Guide's locators, by name, and where each is (for its panel).
pub fn locators_of(guides: &AnimData, part: &str) -> Vec<(String, glam::Vec3)> {
    let Some(rig) = guides.rig.as_deref() else {
        return vec![];
    };
    let w = guides.world_pose(0);
    rig.guides
        .iter()
        .enumerate()
        .filter_map(|(i, g)| {
            g.as_ref()
                .filter(|g| g.part == part)
                .map(|g| (g.name.clone(), w[i].w_axis.truncate()))
        })
        .collect()
}

/// Edits, made of a locator moved to `to` from `from` (autofit's place).
pub fn edit_to(edits: &mut Vec<GuideEdit>, locator: &str, offset: [f32; 3]) {
    match edits.iter_mut().find(|e| e.locator == locator) {
        Some(e) => e.offset = offset,
        None => edits.push(GuideEdit {
            locator: locator.into(),
            offset,
        }),
    }
    edits.retain(|e| e.offset != [0.0; 3]);
}
