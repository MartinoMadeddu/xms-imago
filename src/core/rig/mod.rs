//! Rigging: guides placed on a mesh, a deform skeleton built from them, and
//! skin weights made in steps. After the autorig systems of other packages
//! (mGear's guides and components): what is placed is a handful of guide
//! locators per component, and everything after them is built again from them
//! whenever they move.
//!
//! The steps, each an operator:
//! - **Rig Guide**: one component's locators (a spine, an arm, a leg…), fitted
//!   to the mesh, added to the guides that come in. Components chain.
//! - **Build Skeleton**: the joints, named, placed and oriented for a target
//!   (Unreal, HumanIK, Maya), with each joint's limits and what it is.
//! - **Hard Bind**: every point to one joint, along the surface.
//! - **Envelope**: where one joint's region ends, moved, tilted, grown.
//! - **Range of Motion**: each joint swept within its limits, then poses.
//! - **Delta Mush**, **Bake Weights**: the hard skin smoothed over the range of
//!   motion, and baked into smooth weights.
//!
//! Every joint keeps a **canonical frame**, the same on every target: X to the
//! main child, Y to the pole (the way the joint's apex points when it bends:
//! the elbow back, the knee forward, a knuckle up, the spine forward), Z = X×Y.
//! A hinge bends about Z, the child moving away from the pole (negative Z).
//! Limits and the range of motion are written in it, so they hold whatever
//! the target's axes are.

pub mod bind;
pub mod build;
pub mod fit;
pub mod guides;
pub mod mech;
pub mod mush;
pub mod node;
pub mod presets;
pub mod rigid;
pub mod rom;
pub mod surface;
pub mod view;

use glam::Quat;

/// What a component is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Component {
    Spine,
    /// The neck and the head.
    Neck,
    /// Clavicle, upper arm, forearm, hand.
    Arm,
    /// The fingers and the thumb, on an arm's wrist.
    Hand,
    /// Thigh, calf, foot, ball of the foot.
    Leg,
    /// A four-legged animal's leg: hip (or shoulder), stifle (or elbow), hock
    /// (or knee), fetlock, hoof or paw, toe.
    AnimalLeg,
    /// Joints along a curve: a tail, a tentacle, an antenna, a finger alone.
    Chain,
    /// The rigid frame of a machine: a car's body, a hull, a fuselage. What
    /// the machine's other parts hang from.
    Body,
    /// One joint turning about a line, within limits: a door, a flap, an
    /// aileron, a rudder, a bucket, a robot's axis.
    Hinge,
    /// A joint spinning without end about an axis: a propeller, a rotor, a
    /// turret; blades on it, each pitching about its length.
    Rotor,
    /// A cylinder aimed at its rod's end, the rod sliding in it.
    Piston,
    /// A wheel: suspension sliding up and down, steering if it steers, the
    /// wheel spinning on its axle.
    Wheel,
    /// A gear spinning about its axis; on another gear's centre, it meshes
    /// with it, turning the other way by the ratio of their radii.
    Gear,
    /// A track around two wheels: links that run round the loop.
    Tread,
}

impl Component {
    pub const ALL: [Component; 14] = [
        Component::Spine,
        Component::Neck,
        Component::Arm,
        Component::Hand,
        Component::Leg,
        Component::AnimalLeg,
        Component::Chain,
        Component::Body,
        Component::Hinge,
        Component::Rotor,
        Component::Piston,
        Component::Wheel,
        Component::Gear,
        Component::Tread,
    ];

    /// As in the locators' and the parts' names.
    pub fn word(self) -> &'static str {
        match self {
            Component::Spine => "spine",
            Component::Neck => "neck",
            Component::Arm => "arm",
            Component::Hand => "hand",
            Component::Leg => "leg",
            Component::AnimalLeg => "animalLeg",
            Component::Chain => "chain",
            Component::Body => "body",
            Component::Hinge => "hinge",
            Component::Rotor => "rotor",
            Component::Piston => "piston",
            Component::Wheel => "wheel",
            Component::Gear => "gear",
            Component::Tread => "tread",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Component::Spine => "Spine",
            Component::Neck => "Neck and head",
            Component::Arm => "Arm",
            Component::Hand => "Hand",
            Component::Leg => "Leg",
            Component::AnimalLeg => "Animal leg",
            Component::Chain => "Chain",
            Component::Body => "Body",
            Component::Hinge => "Hinge",
            Component::Rotor => "Rotor",
            Component::Piston => "Piston",
            Component::Wheel => "Wheel",
            Component::Gear => "Gear",
            Component::Tread => "Tread",
        }
    }

    /// A human's part: placed by autofit on a body.
    pub fn human(self) -> bool {
        matches!(
            self,
            Component::Spine | Component::Neck | Component::Arm | Component::Hand | Component::Leg
        )
    }

    /// On the centre line only.
    pub fn centred(self) -> bool {
        matches!(self, Component::Spine | Component::Neck | Component::Body)
    }

    /// Paired: a left and a right one.
    pub fn sided(self) -> bool {
        matches!(
            self,
            Component::Arm
                | Component::Hand
                | Component::Leg
                | Component::AnimalLeg
                | Component::Wheel
                | Component::Tread
        )
    }
}

/// Which side a component is on. Left is +X, the character facing +Z.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum Side {
    #[default]
    Centre,
    Left,
    Right,
}

impl Side {
    /// As in names: C, L, R.
    pub fn letter(self) -> &'static str {
        match self {
            Side::Centre => "C",
            Side::Left => "L",
            Side::Right => "R",
        }
    }

    /// +1 on the left, -1 on the right, +1 in the centre.
    pub fn sign(self) -> f32 {
        if self == Side::Right {
            -1.0
        } else {
            1.0
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Finger {
    Thumb,
    Index,
    Middle,
    Ring,
    Pinky,
}

impl Finger {
    pub const ALL: [Finger; 5] = [
        Finger::Thumb,
        Finger::Index,
        Finger::Middle,
        Finger::Ring,
        Finger::Pinky,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Finger::Thumb => "thumb",
            Finger::Index => "index",
            Finger::Middle => "middle",
            Finger::Ring => "ring",
            Finger::Pinky => "pinky",
        }
    }
}

/// What a joint is, whatever it is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// On the floor under everything; not a deforming joint.
    Root,
    Pelvis,
    /// Spine joints from the pelvis up, from 1.
    Spine(u8),
    /// Neck joints from the shoulders up, from 1.
    Neck(u8),
    Head,
    Clavicle,
    UpperArm,
    LowerArm,
    Hand,
    /// Twist joints along the upper arm, from 1 at the shoulder.
    UpperArmTwist(u8),
    /// Twist joints along the forearm, from 1 at the wrist.
    LowerArmTwist(u8),
    Thigh,
    Calf,
    Foot,
    Ball,
    ThighTwist(u8),
    CalfTwist(u8),
    /// A finger's joints: 0 the metacarpal (in the palm), 1 to 3 the
    /// phalanges from the knuckle out. The thumb has 1 to 3.
    Finger(Finger, u8),
    /// An animal leg's joints from the hip (0) to the hoof (4).
    AnimalLeg(u8),
    /// A chain's joints from its start, from 1.
    Chain(u8),
    /// A machine's frame.
    Body,
    Hinge,
    /// A rotor's hub, spinning.
    Rotor,
    /// A rotor's blade, pitching about its length, from 1.
    Blade(u8),
    PistonBase,
    PistonRod,
    /// A wheel's suspension, sliding along its up.
    Suspension,
    /// A wheel's steering, turning about its up.
    Steer,
    /// The wheel, spinning on its axle.
    Wheel,
    Gear,
    /// A tread's links round its loop, from 1.
    TreadLink(u16),
}

impl Role {
    /// A twist joint: it turns with its bone and bends nothing.
    pub fn is_twist(self) -> bool {
        matches!(
            self,
            Role::UpperArmTwist(_) | Role::LowerArmTwist(_) | Role::ThighTwist(_) | Role::CalfTwist(_)
        )
    }

    /// The joint a twist joint shares its frame and limits with.
    pub fn bone(self) -> Role {
        match self {
            Role::UpperArmTwist(_) => Role::UpperArm,
            Role::LowerArmTwist(_) => Role::LowerArm,
            Role::ThighTwist(_) => Role::Thigh,
            Role::CalfTwist(_) => Role::Calf,
            r => r,
        }
    }
}

/// Rotation orders, as the target reads its rotations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum RotOrder {
    #[default]
    Xyz,
    Yzx,
    Zxy,
    Xzy,
    Yxz,
    Zyx,
}

impl RotOrder {
    pub const ALL: [RotOrder; 6] = [
        RotOrder::Xyz,
        RotOrder::Yzx,
        RotOrder::Zxy,
        RotOrder::Xzy,
        RotOrder::Yxz,
        RotOrder::Zyx,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RotOrder::Xyz => "XYZ",
            RotOrder::Yzx => "YZX",
            RotOrder::Zxy => "ZXY",
            RotOrder::Xzy => "XZY",
            RotOrder::Yxz => "YXZ",
            RotOrder::Zyx => "ZYX",
        }
    }

    /// The rotation of Euler angles (radians) in this order: XYZ turns about
    /// X first, then Y, then Z, as Maya reads it.
    pub fn quat(self, a: [f32; 3]) -> Quat {
        let (x, y, z) = (
            Quat::from_rotation_x(a[0]),
            Quat::from_rotation_y(a[1]),
            Quat::from_rotation_z(a[2]),
        );
        match self {
            RotOrder::Xyz => z * y * x,
            RotOrder::Yzx => x * z * y,
            RotOrder::Zxy => y * x * z,
            RotOrder::Xzy => y * z * x,
            RotOrder::Yxz => z * x * y,
            RotOrder::Zyx => x * y * z,
        }
    }
}

/// How far a joint turns from its rest, about each axis of its canonical
/// frame, in degrees: X twists, Y swings, Z bends; and how far it slides
/// along its X, in scene units (a piston's rod, a suspension).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub slide: [f32; 2],
    /// Spins without end about X (a wheel, a rotor, a gear): a full turn
    /// is its range.
    pub spin: bool,
}

impl Limits {
    pub const fn new(x: [f32; 2], y: [f32; 2], z: [f32; 2]) -> Limits {
        Limits {
            min: [x[0], y[0], z[0]],
            max: [x[1], y[1], z[1]],
            slide: [0.0; 2],
            spin: false,
        }
    }

    /// Turning about Z only, from `min` to `max` degrees.
    pub fn hinge(min: f32, max: f32) -> Limits {
        Limits::new([0.0; 2], [0.0; 2], [min, max])
    }

    /// Spinning about X without end.
    pub fn spinning() -> Limits {
        Limits {
            spin: true,
            ..Limits::new([-180.0, 180.0], [0.0; 2], [0.0; 2])
        }
    }

    /// Sliding along X, from `min` to `max` scene units.
    pub fn sliding(min: f32, max: f32) -> Limits {
        Limits {
            slide: [min, max],
            ..Limits::none()
        }
    }

    /// Fixed: no turn at all.
    pub fn none() -> Limits {
        Limits::new([0.0; 2], [0.0; 2], [0.0; 2])
    }

    /// The other side's: twist and swing mirror, bend stays.
    pub fn mirrored(self) -> Limits {
        Limits {
            min: [-self.max[0], -self.max[1], self.min[2]],
            max: [-self.min[0], -self.min[1], self.max[2]],
            ..self
        }
    }

    /// A slide from the fraction `f` in -1..1.
    pub fn slide_at(&self, f: f32) -> f32 {
        if f >= 0.0 {
            self.slide[1] * f
        } else {
            -self.slide[0] * f
        }
    }

    /// From the fraction `f` in -1..1: -1 the minimum, 1 the maximum, 0 rest.
    pub fn at(&self, axis: usize, f: f32) -> f32 {
        if f >= 0.0 {
            self.max[axis] * f
        } else {
            -self.min[axis] * f
        }
    }
}

/// A human's joint limits, left side, canonical frame, from the ranges of
/// motion clinical references give (rounded). `drop`: how far the arms hang
/// below the horizontal in the rest pose, degrees (0 in a T pose, about 45 in
/// an A pose), which moves the shoulder's range.
pub fn human_limits(role: Role, drop: f32) -> Limits {
    let l = Limits::new;
    match role.bone() {
        Role::Root => Limits::none(),
        Role::Pelvis => l([-30.0, 30.0], [-30.0, 30.0], [-30.0, 30.0]),
        Role::Spine(_) => l([-10.0, 10.0], [-8.0, 8.0], [-8.0, 15.0]),
        Role::Neck(_) => l([-35.0, 35.0], [-25.0, 25.0], [-30.0, 25.0]),
        Role::Head => l([-35.0, 35.0], [-20.0, 20.0], [-25.0, 25.0]),
        Role::Clavicle => l([-10.0, 10.0], [-15.0, 10.0], [-10.0, 25.0]),
        // Bend: forward (-) to 170, back to 50. Swing: up (-) to vertical,
        // down to the body.
        Role::UpperArm => l([-80.0, 80.0], [-(90.0 + drop), drop.max(10.0)], [-170.0, 50.0]),
        Role::LowerArm => l([-80.0, 80.0], [-5.0, 5.0], [-150.0, 5.0]),
        Role::Hand => l([-10.0, 10.0], [-30.0, 20.0], [-70.0, 70.0]),
        Role::Thigh => l([-40.0, 40.0], [-30.0, 45.0], [-20.0, 120.0]),
        Role::Calf => l([-10.0, 10.0], [-3.0, 3.0], [-150.0, 0.0]),
        Role::Foot => l([-20.0, 20.0], [-20.0, 20.0], [-50.0, 20.0]),
        Role::Ball => l([0.0, 0.0], [-5.0, 5.0], [-40.0, 60.0]),
        Role::Finger(Finger::Thumb, 1) => l([-20.0, 20.0], [-30.0, 30.0], [-40.0, 20.0]),
        Role::Finger(Finger::Thumb, 2) => l([-5.0, 5.0], [-10.0, 10.0], [-55.0, 10.0]),
        Role::Finger(Finger::Thumb, _) => l([0.0, 0.0], [-5.0, 5.0], [-80.0, 15.0]),
        Role::Finger(_, 0) => l([0.0, 0.0], [-3.0, 3.0], [-8.0, 2.0]),
        Role::Finger(_, 1) => l([-5.0, 5.0], [-20.0, 20.0], [-90.0, 20.0]),
        Role::Finger(_, 2) => l([0.0, 0.0], [0.0, 0.0], [-110.0, 0.0]),
        Role::Finger(_, _) => l([0.0, 0.0], [0.0, 0.0], [-80.0, 5.0]),
        _ => Limits::none(),
    }
}

/// An animal leg's limits, canonical frame, from the hip (0) to the hoof
/// (4): the stifle and hock bend one way each, the fetlock both ways.
pub fn animal_limits(k: u8) -> Limits {
    let l = Limits::new;
    match k {
        0 => l([-10.0, 10.0], [-15.0, 15.0], [-50.0, 60.0]),
        1 => l([-5.0, 5.0], [-5.0, 5.0], [-90.0, 5.0]),
        2 => l([-5.0, 5.0], [-5.0, 5.0], [-90.0, 5.0]),
        3 => l([0.0, 0.0], [-5.0, 5.0], [-60.0, 40.0]),
        _ => l([0.0, 0.0], [-5.0, 5.0], [-30.0, 30.0]),
    }
}

/// What a machine part or a chain is set to, besides its locators.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MechParams {
    /// A hinge's turn, a chain joint's bend, a blade's pitch, degrees; a
    /// wheel's steering is the larger.
    pub limits: [f32; 2],
    /// A suspension's travel or a piston's stroke, each way, scene units.
    pub travel: f32,
    /// Blades of a rotor, links of a tread.
    pub count: u32,
    /// The wheel steers.
    pub steer: bool,
    /// The primitives the part is fitted to (words or regular expressions
    /// found in their paths, comma-separated: `wheel1`):
    /// its locators are placed from their bounds.
    pub fit: String,
}

impl Default for MechParams {
    fn default() -> Self {
        MechParams::of(Component::Hinge)
    }
}

impl MechParams {
    /// What a new component of a kind starts with.
    pub fn of(c: Component) -> MechParams {
        let (limits, travel, count) = match c {
            Component::Hinge => ([-45.0, 45.0], 0.0, 0),
            Component::Chain => ([-40.0, 40.0], 0.0, 0),
            Component::Rotor => ([-15.0, 15.0], 0.0, 0),
            Component::Piston => ([0.0, 0.0], 0.1, 0),
            Component::Wheel => ([-35.0, 35.0], 0.05, 0),
            Component::Tread => ([0.0, 0.0], 0.0, 40),
            _ => ([0.0, 0.0], 0.0, 0),
        };
        MechParams {
            limits,
            travel,
            count,
            steer: false,
            fit: String::new(),
        }
    }
}

/// One guide locator: whose it is and what it marks.
#[derive(Clone, Debug, PartialEq)]
pub struct GuideTag {
    /// The component's name: `arm_L0`.
    pub part: String,
    /// What the locator marks in it: `elbow`.
    pub name: String,
}

/// One component of a set of guides, and how it is built.
#[derive(Clone, Debug, PartialEq)]
pub struct PartInfo {
    pub component: Component,
    pub side: Side,
    pub index: u32,
    /// The locator it hangs from (`spine_C0_chest`), if any.
    pub attach: Option<String>,
    /// Spine or neck joints.
    pub segments: u32,
    /// Twist joints per bone of an arm or a leg.
    pub twists: u32,
    /// Fingers of a hand, metacarpals in the palm.
    pub fingers: Vec<Finger>,
    pub metacarpals: bool,
    /// Its own word in place of the component's: `aileron` makes
    /// `aileron_L0`. Empty: the component's.
    pub label: String,
    pub mech: MechParams,
}

impl PartInfo {
    /// `arm_L0`, `aileron_L0`.
    pub fn name(&self) -> String {
        let word = if self.label.trim().is_empty() {
            self.component.word()
        } else {
            self.label.trim()
        };
        format!("{}_{}{}", word, self.side.letter(), self.index)
    }
}

/// A joint of a built skeleton, as the rig knows it.
#[derive(Clone, Debug, PartialEq)]
pub struct JointInfo {
    pub role: Role,
    pub side: Side,
    /// The component it was built from (`arm_L0`).
    pub part: String,
    /// The canonical frame in the joint's own space: the joint's world
    /// rotation times this is the canonical frame.
    pub canon: Quat,
    /// None when the limits are off.
    pub limits: Option<Limits>,
    pub order: RotOrder,
    /// Bound to the mesh (the root is not).
    pub deform: bool,
    /// Where its bone ends, in the rest pose: the point it aims at.
    pub tip: glam::Vec3,
    /// A wheel's, a gear's, a rotor's radius; a tread's loop's.
    pub radius: f32,
    /// Spins with another joint (a gear meshing): its X turn is this many
    /// times that joint's.
    pub follow: Option<(usize, f32)>,
    /// A tread link: the loop it runs round.
    pub tread: Option<TreadPath>,
    /// A body's half extents: across, up, along.
    pub size: glam::Vec3,
}

/// A tread's loop in the rest pose, world space: round two wheels of one
/// radius, the link at `start` along it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreadPath {
    pub front: glam::Vec3,
    pub rear: glam::Vec3,
    /// Out of the loop's plane (the axle).
    pub axle: glam::Vec3,
    pub radius: f32,
    pub start: f32,
}

impl TreadPath {
    /// Its length.
    pub fn length(&self) -> f32 {
        2.0 * self.front.distance(self.rear) + std::f32::consts::TAU * self.radius
    }

    /// Where on the loop `s` is along it, and the way along it there: from
    /// the top of the front wheel back along the top, round the rear wheel,
    /// forward along the bottom, round the front wheel.
    pub fn at(&self, s: f32) -> (glam::Vec3, glam::Vec3) {
        let len = self.length();
        let s = s.rem_euclid(len.max(1e-6));
        let back = (self.rear - self.front).normalize_or(glam::Vec3::NEG_Z);
        let up = self.axle.cross(back).normalize_or(glam::Vec3::Y);
        let up = if up.y < 0.0 { -up } else { up };
        let straight = self.front.distance(self.rear);
        let half = std::f32::consts::PI * self.radius;
        let r = self.radius;
        if s < straight {
            (self.front + up * r + back * s, back)
        } else if s < straight + half {
            let a = (s - straight) / r.max(1e-6);
            (
                self.rear + (up * a.cos() + back * a.sin()) * r,
                back * a.cos() - up * a.sin(),
            )
        } else if s < 2.0 * straight + half {
            (self.rear - up * r - back * (s - straight - half), -back)
        } else {
            let a = (s - 2.0 * straight - half) / r.max(1e-6);
            (
                self.front - (up * a.cos() + back * a.sin()) * r,
                up * a.sin() - back * a.cos(),
            )
        }
    }
}

/// What the rig knows about a clip's joints: guides on a guide clip, joints on
/// a built skeleton. Indexed as the clip's joints.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RigInfo {
    pub parts: Vec<PartInfo>,
    /// Per joint of a guide clip.
    pub guides: Vec<Option<GuideTag>>,
    /// Per joint of a built skeleton.
    pub joints: Vec<JointInfo>,
}

impl RigInfo {
    /// The joint of `role` on `side`.
    #[cfg(test)]
    pub fn joint(&self, role: Role, side: Side) -> Option<usize> {
        self.joints.iter().position(|j| j.role == role && j.side == side)
    }
}

/// A locator's full name: `arm_L0_elbow`.
pub fn locator_name(part: &str, name: &str) -> String {
    format!("{part}_{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrored limits swap and negate twist and swing, keep bend; `at` reads
    /// a fraction of either end; an order composes in the order it says.
    #[test]
    fn limits_mirror_and_orders_compose() {
        let l = human_limits(Role::UpperArm, 45.0);
        let m = l.mirrored();
        assert_eq!(m.min[1], -l.max[1]);
        assert_eq!(m.max[1], -l.min[1]);
        assert_eq!(m.min[2], l.min[2]);
        assert_eq!(l.at(2, -1.0), -170.0);
        assert_eq!(l.at(2, 0.5), 25.0);
        // XYZ turns about X first: Y goes to Z, which Z leaves. ZYX turns
        // about Z first: Y goes to -X, which X leaves.
        let quarter = [std::f32::consts::FRAC_PI_2, 0.0, std::f32::consts::FRAC_PI_2];
        let v = RotOrder::Xyz.quat(quarter) * glam::Vec3::Y;
        assert!((v - glam::Vec3::Z).length() < 1e-5, "{v}");
        let v = RotOrder::Zyx.quat(quarter) * glam::Vec3::Y;
        assert!((v - glam::Vec3::NEG_X).length() < 1e-5, "{v}");
        assert_eq!(locator_name("arm_L0", "elbow"), "arm_L0_elbow");
    }
}
