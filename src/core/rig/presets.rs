//! Targets a skeleton is built for: what each joint is called, how its axes
//! lie, the rotation order, whether there is a root joint. Choosing one on
//! Build Skeleton fills in its axes, order and root; each can then be changed
//! on its own.
//!
//! - **Unreal**: the Unreal Engine 5 and MetaHuman skeleton (`pelvis`,
//!   `spine_01`, `upperarm_l`, `index_metacarpal_l`…). X along the bone, a
//!   root on the floor. Its axes are the MetaHuman's as measured on the
//!   example character: the spine's Y backwards, the left arm's X to the child
//!   and the right arm's to the parent, the legs the other way round (the left
//!   leg's X to the parent), the foot lined up with the calf.
//! - **HumanIK**: Autodesk's HumanIK names (`Hips`, `LeftArm`, `LeftForeArm`,
//!   `LeftHandIndex1`, `LeftInHandIndex` for a metacarpal, `LeftArmRoll` for a
//!   twist). Every joint's axes lined up with the world, as HumanIK's own
//!   skeletons are made; no root.
//! - **Maya**: the naming rule of mGear's components,
//!   `{component}_{side}{index}_{name}_jnt` (`arm_L0_elbow_jnt`). X along the
//!   bone, Y to the pole; the right side mirrored in behaviour (its X and Y
//!   reversed, so the same rotation bends both sides alike). No root.

use glam::{Mat3, Vec3};

use super::{Finger, Role, RotOrder, Side};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum Preset {
    #[default]
    Unreal,
    HumanIk,
    Maya,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Unreal, Preset::HumanIk, Preset::Maya];

    pub fn label(self) -> &'static str {
        match self {
            Preset::Unreal => "Unreal (MetaHuman)",
            Preset::HumanIk => "HumanIK",
            Preset::Maya => "Maya",
        }
    }

    /// What choosing it sets: the rotation order and whether there is a root.
    pub fn order(self) -> RotOrder {
        RotOrder::Xyz
    }

    pub fn root(self) -> bool {
        self == Preset::Unreal
    }

    /// The axes a Custom choice starts from.
    pub fn custom(self) -> (Axis, Axis, bool) {
        match self {
            Preset::Unreal | Preset::Maya => (Axis::X, Axis::Y, true),
            Preset::HumanIk => (Axis::X, Axis::Y, false),
        }
    }
}

/// A joint axis, signed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Axis {
    X,
    Y,
    Z,
    NegX,
    NegY,
    NegZ,
}

impl Axis {
    pub const ALL: [Axis; 6] = [Axis::X, Axis::Y, Axis::Z, Axis::NegX, Axis::NegY, Axis::NegZ];

    pub fn label(self) -> &'static str {
        match self {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
            Axis::NegX => "-X",
            Axis::NegY => "-Y",
            Axis::NegZ => "-Z",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Axis::X | Axis::NegX => 0,
            Axis::Y | Axis::NegY => 1,
            Axis::Z | Axis::NegZ => 2,
        }
    }

    pub fn sign(self) -> f32 {
        match self {
            Axis::X | Axis::Y | Axis::Z => 1.0,
            _ => -1.0,
        }
    }
}

/// How a joint's axes are made from its canonical frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Orient {
    /// Lined up with the world.
    World,
    /// These axes, whatever the bone.
    Fixed(Mat3),
    /// `aim` along the canonical X (the bone), `up` along the canonical Y
    /// (the pole); on `flip`'s side both reversed.
    Frame { aim: Axis, up: Axis, flip: Option<Side> },
    /// The parent's axes.
    Parent,
}

/// A frame whose `aim` axis is `x` and `up` axis is `y`, right-handed.
pub fn frame(aim: Axis, up: Axis, x: Vec3, y: Vec3) -> Option<Mat3> {
    let (i, j) = (aim.index(), up.index());
    if i == j {
        return None;
    }
    let mut cols = [Vec3::ZERO; 3];
    cols[i] = x * aim.sign();
    cols[j] = y * up.sign();
    let k = 3 - i - j;
    // Cyclic (i, j, k) means col k = col i × col j.
    cols[k] = if (i + 1) % 3 == j {
        cols[i].cross(cols[j])
    } else {
        cols[j].cross(cols[i])
    };
    Some(Mat3::from_cols(cols[0], cols[1], cols[2]))
}

/// How a joint of `role` is oriented for a target.
pub fn orient(preset: Preset, role: Role) -> Orient {
    use Axis::*;
    match preset {
        Preset::HumanIk => Orient::World,
        Preset::Maya => match role {
            Role::Root => Orient::World,
            _ => Orient::Frame {
                aim: X,
                up: Y,
                flip: Some(Side::Right),
            },
        },
        Preset::Unreal => match role.bone() {
            // X across, Y back, Z up.
            Role::Root => Orient::Fixed(Mat3::from_cols(Vec3::X, Vec3::NEG_Z, Vec3::Y)),
            Role::Pelvis | Role::Spine(_) | Role::Neck(_) | Role::Head => Orient::Frame {
                aim: X,
                up: NegY,
                flip: None,
            },
            Role::Clavicle | Role::UpperArm | Role::LowerArm => Orient::Frame {
                aim: X,
                up: Y,
                flip: Some(Side::Right),
            },
            Role::Hand | Role::Finger(..) => Orient::Frame {
                aim: X,
                up: NegY,
                flip: Some(Side::Right),
            },
            Role::Thigh | Role::Calf => Orient::Frame {
                aim: X,
                up: Y,
                flip: Some(Side::Left),
            },
            Role::Foot => Orient::Parent,
            Role::Ball => Orient::Frame {
                aim: X,
                up: Y,
                flip: Some(Side::Right),
            },
            _ => Orient::Frame {
                aim: X,
                up: Y,
                flip: None,
            },
        },
    }
}

/// Two digits from 1: `01`.
fn nn(k: u8) -> String {
    format!("{k:02}")
}

/// A joint's name on a target. `part`: its component's name (`arm_L0`), for
/// the Maya rule.
pub fn name(preset: Preset, role: Role, side: Side, part: &str) -> String {
    match preset {
        Preset::Unreal => unreal(role, side),
        Preset::HumanIk => human_ik(role, side),
        Preset::Maya => format!("{part}_{}_jnt", maya(role)),
    }
}

fn unreal(role: Role, side: Side) -> String {
    let s = match side {
        Side::Right => "_r",
        _ => "_l",
    };
    match role {
        Role::Root => "root".into(),
        Role::Pelvis => "pelvis".into(),
        Role::Spine(k) => format!("spine_{}", nn(k)),
        Role::Neck(k) => format!("neck_{}", nn(k)),
        Role::Head => "head".into(),
        Role::Clavicle => format!("clavicle{s}"),
        Role::UpperArm => format!("upperarm{s}"),
        Role::LowerArm => format!("lowerarm{s}"),
        Role::Hand => format!("hand{s}"),
        Role::UpperArmTwist(k) => format!("upperarm_twist_{}{s}", nn(k)),
        Role::LowerArmTwist(k) => format!("lowerarm_twist_{}{s}", nn(k)),
        Role::Thigh => format!("thigh{s}"),
        Role::Calf => format!("calf{s}"),
        Role::Foot => format!("foot{s}"),
        Role::Ball => format!("ball{s}"),
        Role::ThighTwist(k) => format!("thigh_twist_{}{s}", nn(k)),
        Role::CalfTwist(k) => format!("calf_twist_{}{s}", nn(k)),
        Role::Finger(f, 0) => format!("{}_metacarpal{s}", f.word()),
        Role::Finger(f, k) => format!("{}_{}{s}", f.word(), nn(k)),
    }
}

fn human_ik(role: Role, side: Side) -> String {
    let s = match side {
        Side::Right => "Right",
        _ => "Left",
    };
    let finger = |f: Finger| match f {
        Finger::Thumb => "Thumb",
        Finger::Index => "Index",
        Finger::Middle => "Middle",
        Finger::Ring => "Ring",
        Finger::Pinky => "Pinky",
    };
    // A first roll joint, then leaf rolls.
    let roll = |base: &str, k: u8| {
        if k == 1 {
            format!("{s}{base}Roll")
        } else {
            format!("Leaf{s}{base}Roll{}", k - 1)
        }
    };
    match role {
        Role::Root => "Reference".into(),
        Role::Pelvis => "Hips".into(),
        Role::Spine(1) => "Spine".into(),
        Role::Spine(k) => format!("Spine{}", k - 1),
        Role::Neck(1) => "Neck".into(),
        Role::Neck(k) => format!("Neck{}", k - 1),
        Role::Head => "Head".into(),
        Role::Clavicle => format!("{s}Shoulder"),
        Role::UpperArm => format!("{s}Arm"),
        Role::LowerArm => format!("{s}ForeArm"),
        Role::Hand => format!("{s}Hand"),
        Role::UpperArmTwist(k) => roll("Arm", k),
        Role::LowerArmTwist(k) => roll("ForeArm", k),
        Role::Thigh => format!("{s}UpLeg"),
        Role::Calf => format!("{s}Leg"),
        Role::Foot => format!("{s}Foot"),
        Role::Ball => format!("{s}ToeBase"),
        Role::ThighTwist(k) => roll("UpLeg", k),
        Role::CalfTwist(k) => roll("Leg", k),
        Role::Finger(f, 0) => format!("{s}InHand{}", finger(f)),
        Role::Finger(f, k) => format!("{s}Hand{}{k}", finger(f)),
    }
}

fn maya(role: Role) -> String {
    match role {
        Role::Root => "root".into(),
        Role::Pelvis => "pelvis".into(),
        Role::Spine(k) => format!("{}", k - 1),
        Role::Neck(k) => format!("{}", k - 1),
        Role::Head => "head".into(),
        Role::Clavicle => "clavicle".into(),
        Role::UpperArm => "upperarm".into(),
        Role::LowerArm => "forearm".into(),
        Role::Hand => "hand".into(),
        Role::UpperArmTwist(k) => format!("upperarm_twist{}", k - 1),
        Role::LowerArmTwist(k) => format!("forearm_twist{}", k - 1),
        Role::Thigh => "thigh".into(),
        Role::Calf => "calf".into(),
        Role::Foot => "foot".into(),
        Role::Ball => "ball".into(),
        Role::ThighTwist(k) => format!("thigh_twist{}", k - 1),
        Role::CalfTwist(k) => format!("calf_twist{}", k - 1),
        Role::Finger(f, k) => format!("{}{k}", f.word()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names on each target; a frame from aim and up is right-handed
    /// whichever axes they are.
    #[test]
    fn names_and_frames() {
        assert_eq!(
            name(Preset::Unreal, Role::Finger(Finger::Index, 0), Side::Left, "hand_L0"),
            "index_metacarpal_l"
        );
        assert_eq!(
            name(Preset::Unreal, Role::Spine(3), Side::Centre, "spine_C0"),
            "spine_03"
        );
        assert_eq!(
            name(Preset::HumanIk, Role::Spine(2), Side::Centre, "spine_C0"),
            "Spine1"
        );
        assert_eq!(
            name(Preset::HumanIk, Role::UpperArmTwist(1), Side::Right, "arm_R0"),
            "RightArmRoll"
        );
        assert_eq!(
            name(Preset::HumanIk, Role::Finger(Finger::Ring, 0), Side::Left, "hand_L0"),
            "LeftInHandRing"
        );
        assert_eq!(
            name(Preset::Maya, Role::LowerArm, Side::Left, "arm_L0"),
            "arm_L0_forearm_jnt"
        );
        for aim in Axis::ALL {
            for up in Axis::ALL {
                let Some(f) = frame(aim, up, Vec3::X, Vec3::Y) else {
                    assert_eq!(aim.index(), up.index());
                    continue;
                };
                assert!((f.determinant() - 1.0).abs() < 1e-5, "{aim:?} {up:?}");
                assert!((f.col(aim.index()) * aim.sign() - Vec3::X).length() < 1e-6);
                assert!((f.col(up.index()) * up.sign() - Vec3::Y).length() < 1e-6);
            }
        }
    }
}
