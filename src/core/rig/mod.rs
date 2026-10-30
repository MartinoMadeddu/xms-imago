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
pub mod mush;
pub mod node;
pub mod presets;
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
}

impl Component {
    pub const ALL: [Component; 5] = [
        Component::Spine,
        Component::Neck,
        Component::Arm,
        Component::Hand,
        Component::Leg,
    ];

    /// As in the locators' and the parts' names.
    pub fn word(self) -> &'static str {
        match self {
            Component::Spine => "spine",
            Component::Neck => "neck",
            Component::Arm => "arm",
            Component::Hand => "hand",
            Component::Leg => "leg",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Component::Spine => "Spine",
            Component::Neck => "Neck and head",
            Component::Arm => "Arm",
            Component::Hand => "Hand",
            Component::Leg => "Leg",
        }
    }

    /// Paired: a left and a right one.
    pub fn sided(self) -> bool {
        matches!(self, Component::Arm | Component::Hand | Component::Leg)
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
/// frame, in degrees: X twists, Y swings, Z bends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Limits {
    const fn new(x: [f32; 2], y: [f32; 2], z: [f32; 2]) -> Limits {
        Limits {
            min: [x[0], y[0], z[0]],
            max: [x[1], y[1], z[1]],
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
}

impl PartInfo {
    /// `arm_L0`.
    pub fn name(&self) -> String {
        format!("{}_{}{}", self.component.word(), self.side.letter(), self.index)
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
