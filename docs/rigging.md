# Rigging

A deform rig built from guides, regeneratable: guide locators are placed on the
mesh (autofit places them, you move them after), the joints are built from them
named and oriented for a target, and the skin weights are made in steps, from a
hard bind to smooth weights. Change a guide and everything after it is built
again. After the autorig systems of other packages (mGear's guides and
components), as operators of the scene graph.

![Guides fitted to the example character, the hard bind's regions, a baked
joint's weights (thigh_l), and the range of motion's squat with the baked
weights](xms_rig_human.png)

The template **Rig: full human** (File > Templates > Simon > Rigging) does it
all on the example character's mesh (`ragdoll/split/into_the_car_.fbx`, a
MetaHuman body without its head; its own skeleton is not used):

```
Character (Load FBX) ─ Geometry ─┬─ Full Human (8 Rig Guides) ─ Skeleton (Build Skeleton)
                                 │                                 │
                                 └─ HardBind ◄─────────────────────┤
                                       │            RangeOfMotion ◄┤
                                       ├─ DeltaMush ◄──────────────┤
                                       └─ BakeWeights ◄────────────┘
                                             └─ Skin (geometry, skeleton, range of motion)
```

Press Space on Skin to play the range of motion with the baked weights. View
HardBind, DeltaMush or BakeWeights to see the weights painted on the mesh.

## The operators

All in the add menu's **Rigging** group.

| Operator | Inputs | Gives |
|---|---|---|
| **Spine / Neck / Arm / Hand / Leg Guide** (Rig Guide) | Mesh, Guides | the guides coming in, with this component's locators added |
| **Build Skeleton** | Guides | the deform skeleton |
| **Hard Bind** | Geometry, Skeleton | the geometry, every point weighted 1 to one joint |
| **Envelope** | Geometry, Skeleton | the hard weights with joints' regions moved |
| **Range of Motion** | Skeleton | a clip of every joint moving within its limits |
| **Delta Mush** | Geometry, Rest, Clip | the geometry skinned at the playhead and mushed |
| **Bake Weights** | Geometry, Rest, Clip | the geometry with smooth weights |

Weights are point attributes, as Load FBX and Capture write them
(`skel:joint0` to `skel:joint3`, `skel:jointWeights`), so the Skin operator and
Write FBX take them as they take any.

## Guides

A **guide** is one component's set of **locators**, named
`{component}_{side}{index}_{locator}` (`arm_L0_elbow`). Components chain: each
Rig Guide adds its locators to the guides wired into its second input, its
first locator hanging from the locator it **attaches to** (empty: the usual one,
greyed in the field). The guides are a clip, one joint per locator, so the
Viewer shows them as a skeleton over the mesh.

| Component | Locators | Attaches to |
|---|---|---|
| Spine | hips, mid, chest | nothing |
| Neck and head | neck, head, head_end | `spine_C0_chest` |
| Arm | clavicle, shoulder, elbow, wrist, hand_end, pole | `spine_C0_chest` |
| Hand | per finger: meta (not the thumb), 1, 2, 3, end | `arm_{side}{index}_wrist` |
| Leg | hip, knee, ankle, ball, toe, heel, pole | `spine_C0_hips` |

`pole` (where the elbow or knee points) and `heel` are not on the joint chain:
they are drawn as a line from their joint. They, the ends and every locator's
role are kept with the guides for a control rig to be built from later; this
first step builds the deform skeleton only.

Each component's parameters: side, index, attach, the number of spine or neck
joints, twist joints per bone (arms and legs), which fingers and whether they
have metacarpals (hands). **Locators** lists every locator, where it is and how
far it was moved from where the fit put it, in centimetres; **Back to the fit**
clears the moves. Locators are moved there, by their numbers; they cannot be
dragged in the Viewer yet.

The **Full Human** compound in the template is the eight guides (spine, neck,
both arms, both hands, both legs) packed with Make Compound: double-click it to
see them.

### Autofit

On by default. The mesh on the first input is measured, standing on the floor,
Y up, facing +Z, its left at +X, in an A or a T pose. It is cut into horizontal
slices, each falling apart into pieces where there are gaps across X:

- the **crotch** is the lowest slice whose middle piece spans X = 0;
- each **leg**'s centre line runs through its piece in each slice below the
  crotch, the foot's toe and heel are its front and back on the floor;
- each **arm** is a piece of its own about mid torso, followed slice by slice up
  to where it joins the torso (the **armpit**) and down to the fingertips; in a
  T pose, where the torso suddenly widens;
- the **neck** is where the torso narrows above the shoulders; a mesh that ends
  there has no head, and the head is placed above it;
- the arm's points binned by their distance from the shoulder give its centre
  line; the hand's points give the hand's frame (along the hand, the back of
  the hand, across).

Joints are placed on those in the proportions of a reference body (the example
character's), and the fingers from that reference hand, relaxed, in the
measured hand's frame. Without a mesh, or with Autofit off, the locators go
where the reference body has them.

What was measured: on the example character, built for Unreal, the 80 joints
land a mean 0.9 cm and at most 3.6 cm (spine_04) from the character's own joints
of the same names, every bone's X axis within 18 degrees of the character's (a
test checks it). The proportions came from that same body, so this says the
fit finds what it was made from; it has not been tried on other bodies, on a T
pose, or on a mesh with a head. Fingers are where the reference hand has them,
not found on the mesh: on a hand posed differently they need moving.

## Build Skeleton

Joints are made where their locators are: spine joints spread along the curve
through hips, mid and chest, neck joints from the neck to the head, twist joints
along their bone. Each joint aims at the next; its pole axis turns toward the
pole (the elbow's back, the knee's front, the back of the hand, forward for the
spine and neck).

Every joint keeps a **canonical frame**, whatever the target: X to its main
child, Y toward the pole, Z = X×Y. A hinge bends about Z, the child moving away
from the pole. Limits and the range of motion are written in it, so they hold
on every target. The right side is the left's mirror image: the same fraction of
its limits bends both sides alike.

### Targets

Choosing a **Target** sets the names, the axes, the rotation order and the root
joint; **Axes**, **Rotation order**, **Root joint** and **Limits** can then be
changed on their own.

| Target | Names | Axes | Root |
|---|---|---|---|
| **Unreal (MetaHuman)** | `pelvis`, `spine_01`…, `neck_01`…, `clavicle_l`, `upperarm_l`, `lowerarm_l`, `hand_l`, `index_metacarpal_l`, `index_01_l`, `thigh_l`, `calf_l`, `foot_l`, `ball_l`, `upperarm_twist_01_l`… | as measured on the example MetaHuman: X along the bone; the spine's Y backwards; the left arm's X to the child, the right's to the parent; the legs the other way round (the left leg's X to the parent); the foot lined up with the calf | yes |
| **HumanIK** | `Hips`, `Spine`, `Spine1`…, `LeftShoulder`, `LeftArm`, `LeftForeArm`, `LeftHand`, `LeftInHandIndex`, `LeftHandIndex1`, `LeftUpLeg`, `LeftLeg`, `LeftFoot`, `LeftToeBase`, `LeftArmRoll`, `LeafLeftArmRoll1`… | every joint lined up with the world | no |
| **Maya** | mGear's naming rule, `{component}_{side}{index}_{name}_jnt`: `arm_L0_forearm_jnt`, `spine_C0_0_jnt` | X along the bone, Y to the pole, the right side mirrored in behaviour (its X and Y reversed) | no |

![Build Skeleton's parameters: the target, the axes as the target lays them
out, the skeleton found as a human](xms_rig_build.png)

**Custom** axes put one rule on every joint: the axis along the bone, the axis
toward the pole, and whether the right side is mirrored. The Unreal names and
axes were checked against the example character, joint for joint; the HumanIK
and Maya names follow those packages' conventions and have not been loaded into
them. Unreal's hand is aimed at the middle finger's knuckle here, where the
MetaHuman's lines up with the forearm.

The rotation order is kept with each joint and used by Range of Motion. Write
FBX does not write it yet.

The skeleton says which joint is which part of a human, so Characterize,
Retarget, Reach and Body Collide take it as they take any.

### Limits

Each joint's turn from its rest about its canonical axes, in degrees, from the
ranges of motion clinical references give (rounded): an elbow bends to 150 and
not backwards, a knee to 150 and not backwards, a hip 120 forward and 20 back,
an ankle 20 up and 50 down, a knuckle 90, and so on. The shoulder's range moves
with how far the arms hang in the rest pose (an A pose's 45 degrees, a T
pose's 0). They are used by Range of Motion; nothing clamps a clip to them yet.
Turned off, Range of Motion turns every joint a right angle either way.

## Weights

Weights are made in steps, each its own operator.

**Hard Bind.** Each deforming joint's bone is a segment, from the joint to the
point it aims at. A point whose nearest bone is close to it (within 1.6 times
that bone's nearest distance to the mesh) seeds that bone's region; the regions
then grow across the surface from their seeds, each point going to the region
that reaches it first. Growing along the surface, not through space, keeps a
hand resting on a thigh off the thigh, and fingers off each other. A bone with
twist joints is shared between them along its length: each twist joint takes the
stretch around where it is, the bone's own joint the stretch at its end, so
that when the bone twists the skin twists more toward its end (see Range of
Motion). That is this rig's convention, and not how Unreal's own MetaHumans are
weighted.

**Envelope.** Per joint, where its region meets the one before it (the bone that
ends where it starts, or its parent) is a plane across its bone at the joint.
**Slide** moves it along the bone, **Tilt** and **Tilt across** turn it about
the joint's pole axis and third axis, **Grow** takes points from the
neighbouring regions along the surface (or gives them back). The weights stay 0
or 1.

**Range of Motion.** First every joint alone, both sides at once, about each
axis it can turn about: to the most it bends one way, back, the other way, back.
Then poses: arms up, out, forward, back, a hug, hands to shoulders, arms turned
in and out, wrists, a fist, the hand open, bending forward and back, leaning,
twisting, looking around, a squat, sitting, lunges, a knee up, legs apart and
crossed, on tiptoe. Every angle is a fraction of the joint's limits, so the
poses follow the limits when they change. With the defaults (8 frames from key
to key) a whole human is 2,097 frames; the template sets the scene's range to
them. When a bone with twist joints twists, each twist joint turns back by the
part of the twist that does not reach it, so the skin turns more toward the
bone's end. Its Parameters list each part and its frames.

**Delta Mush.** The weighted geometry skinned to the clip at the playhead,
smoothed, and its detail put back in the smoothed surface's frames
(Mancewicz et al., 2014): a hard skin's seams smoothed out, the shape kept.
**Smoothing** passes (20) and **Step** (0.5).

**Bake Weights.** The hard skin moved through the clip (at **Frames used**
frames, spread over it) and delta-mushed is the target. For each point, the
joints of the regions within a few edges of it are the candidates (at most six);
the blend of them that follows the target best over all those frames, every
weight at least zero and summing to one, is solved exactly (every subset of the
candidates tried), and the four heaviest kept. Its Parameters say how far the
baked skin is from the mushed one, and how far the hard skin was.

What was measured on the template, on a 2-core cloud machine: hard bind of
32,334 points to 77 joints, 54 ms; the range of motion, 10 ms; the bake over 120
frames, 1.6 s. The baked skin is a mean 0.04 mm from the mushed skin over those
frames, at most 3.7 cm (in the spine_01 region, where the thighs swing); the
hard skin was a mean 0.15 mm and at most 21 cm from it. 15,072 points end up
blended (a second joint over 5%). The squat in the picture above was looked at; the weights have not
been taken into another package.

### Seeing the weights

While Hard Bind, Envelope, Delta Mush or Bake Weights is viewed, the Viewer paints
the geometry: each joint's region in a colour of its own (the joint each point
weighs most to), or, with a joint picked in **Show**, that joint's weights from
dark blue (none) through red to yellow (all). The choice is the Viewer's: it
cooks nothing again.

## Not done yet

- Components beyond a human: animal legs, pistons, gears, wheels, a car, an
  aeroplane, a helicopter, tank treads
- Control rigs on the deform skeleton (the guides keep what they need: roles,
  poles, heels, ends, limits)
- Dragging locators in the Viewer; mirroring guides from one side to the other
- A clip held within the limits; the rotation order written to FBX
- Autofit tried on other bodies, on a T pose, and on a mesh with a head
