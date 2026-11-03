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

Machines, animals and chains are rigged the same way, from their own
components: a car, a tractor, an aeroplane, a helicopter, a tank, gears and a
piston, a robot arm, a quadruped, a tentacle (see [Templates](#templates)). A
machine's pieces are bound whole to a joint each (Rigid Bind) and moved whole
(Rigid Skin), keeping their materials:

```
Model (Load USD) ─ Scene ─┬─ Car (Body and 4 Wheel Guides) ─ Skeleton ─┬─ RangeOfMotion
                          └─ RigidBind ◄───────────────────────────────┤        │
                                └─ RigidSkin (geometry, skeleton, range of motion)
```

![The DeLorean steering while it drives, the mini car kit's tractor raising its
shovel, an aeroplane with its control surfaces at their limits, a helicopter's
rotors](xms_rig_vehicles.png)

![A tank's turret, gun and treads, three meshing gears and a piston, a robot
arm, a quadruped folding its legs](xms_rig_machines.png)

## The operators

All in the add menu's **Rigging** group.

| Operator | Inputs | Gives |
|---|---|---|
| **Spine / Neck / Arm / Hand / Leg Guide**, **Animal Leg / Chain / Body / Hinge / Rotor / Piston / Wheel / Gear / Tread Guide** (Rig Guide) | Mesh, Guides | the guides coming in, with this component's locators added |
| **Build Skeleton** | Guides | the deform skeleton |
| **Hard Bind** | Geometry, Skeleton | the geometry, every point weighted 1 to one joint |
| **Envelope** | Geometry, Skeleton | the hard weights with joints' regions moved |
| **Range of Motion** | Skeleton | a clip of every joint moving within its limits |
| **Delta Mush** | Geometry, Rest, Clip | the geometry skinned at the playhead and mushed |
| **Bake Weights** | Geometry, Rest, Clip | the geometry with smooth weights |
| **Rigid Bind** | Geometry, Skeleton | each primitive bound whole to one joint |
| **Rigid Skin** | Geometry, Rest, Clip | the bound primitives moved by their joints at the playhead |
| **Rig Proxy** | Skeleton | a simple shape per joint, bound to it, for a skeleton with no mesh |

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
| Animal leg | hip, knee, hock, fetlock, hoof, toe | nothing |
| Chain | start, mid, end, pole | nothing |
| Body | centre, front, side, top | nothing |
| Hinge | pivot, axis, tip | `body_C0_centre` |
| Rotor | centre, axis, tip | `body_C0_centre` |
| Piston | base, end | `body_C0_centre` |
| Wheel | centre, axle, top | `body_C0_centre` |
| Gear | centre, axis, tip | `body_C0_centre` |
| Tread | front, rear, top | `body_C0_centre` |

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

### Machine parts, animal legs and chains

Each makes its joints this way:

| Component | Joints | Moves |
|---|---|---|
| Animal leg | hip, stifle, hock, fetlock, hoof (at the front: shoulder, elbow, knee, fetlock, hoof, by the same names) | each bends the way its apex points between its neighbours; the stifle and hock one way, the fetlock both |
| Chain | as many as **Joints**, along the curve through start, mid and end | each bends within **Bend**, swings half as far, twists 10 degrees, its Y toward the pole |
| Body | one, at its centre, facing its front | rolls 4 degrees, pitches 3: a machine's frame; its front, side and top give Rig Proxy its box |
| Hinge | one at the pivot, turning about the line from the pivot to the axis locator | within **Turn**. A hinge of 0 to 0 is a fixed piece: a wing, a fin |
| Rotor | a hub at the centre spinning about the axis; **Blades** round it, each pitching about its length within **Pitch** | the hub spins without end; no blades makes a turret or a turntable |
| Piston | a cylinder at the base aimed at the end; a rod at the end sliding along it | the rod slides **Stroke** either way |
| Wheel | a suspension (if **Suspension** is more than 0), a steering (if it **Steers**), the wheel spinning on its axle | the suspension slides, the steering turns within **Steering**, the wheel spins by the distance driven over its radius |
| Gear | one at the centre spinning about the axis; its radius to the tip | on another gear's centre it meshes: it hangs from what that gear hangs from and turns the other way by the ratio of their radii |
| Tread | **Links** round the loop around two wheels: the front and rear centres, the top giving the radius | the links run round the loop by the distance driven |

A part can be given its own **Name** (`aileron` makes `aileron_L0`, its joints
`aileron_l0_hinge`): parts of one name on both sides move together in the
range of motion. A wheel's steering turns both sides the same way; every other
right-side twist and swing is the left's mirror image.

Machine parts are not autofitted from a body: each names the primitives it is
in **Fit** (words or regular expressions found in their paths,
comma-separated), and its locators are placed from their bounds: a wheel's
centre, its axle out through its thin side away from what it hangs from, its
radius; a hinge's line along the box's longest side, on the side nearest what
it hangs from; a rotor's or a gear's axis through its thin side; a piston along
its length; a tread round two wheels as high as its box; a body at the centre
of its box, facing +Z. Without a match, or with Fit off, the locators go where
a reference part has them (a metre or so across), and are moved from there.
Locators of machine parts are listed in scene units.

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

### Names of machine parts

Machine parts, animal legs and chains take the part's name and the joint's,
in the target's way: `wheel_l0_spin` (Unreal, lower case), `wheel_L0_spin`
(HumanIK), `wheel_L0_spin_jnt` (Maya). The joints' names in a part:
`body`, `hinge`, `hub` and `blade_01`…, `base` and `rod`, `suspension`,
`steer` and `spin`, `gear`, `link_001`…, `01`… for a chain, `hip`, `stifle`,
`hock`, `fetlock` and `hoof` for an animal leg. The machine parts' axes on
Unreal are X along the part, Y to its pole, not mirrored on the right; on
Maya, mirrored in behaviour as a human's are; on HumanIK, lined up with the
world.

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

### Rigid parts

**Rigid Bind** gives each primitive whole to one joint, every point weight one:
by rows (the primitives whose paths match a row's words go to its joint, or to
a part: its wheel if it has one, else its first joint; the first row that
matches wins), else to the joint whose bone is nearest the middle of the
primitive's bounds. **Rigid Skin** moves a primitive bound whole to one joint
by its placement: its geometry, materials and textures stay as they are and
nothing is copied, so the 460,000-triangle DeLorean moves by 43 matrices a
frame. A primitive whose points weigh on several joints is skinned point by
point. **Rig Proxy** makes simple shapes for a skeleton without a mesh, each its
own primitive (`/proxy/<joint>`) bound whole to its joint and coloured by what
the joint is: a box for a body, a disc for a wheel with a bar across it (to see
it turn), blades, a plate for a hinge, a cylinder and a rod for a piston, a
toothed disc for a gear, a block for each tread link, a box along every other
bone.

### Seeing the weights

While Hard Bind, Envelope, Delta Mush, Bake Weights or Rigid Bind is viewed, the Viewer paints
the geometry: each joint's region in a colour of its own (the joint each point
weighs most to), or, with a joint picked in **Show**, that joint's weights from
dark blue (none) through red to yellow (all). The choice is the Viewer's: it
cooks nothing again.

## Templates

All under File > Templates > Simon > Rigging; each opens playing nothing: press
Space. Each sets the scene's range to its range of motion.

| Template | Rig | Seen on |
|---|---|---|
| Rig: full human | spine, neck, arms, hands, legs (Full Human compound), baked weights | the example character |
| Rig: human leg | spine and left leg, baked weights | the example character |
| Rig: human arm and hand | spine, left arm, left hand, baked weights | the example character |
| Rig: spine and neck | spine, neck and head, baked weights | the example character |
| Rig: car (DeLorean) | body, four wheels with suspension, the fronts steering, fitted to the model's primitives; the callipers steer but do not spin | the DeLorean, its 43 primitives bound whole |
| Rig: tractor (mini car kit) | body, four wheels of two sizes, the shovel on a hinge, fitted to the kit's primitives | the mini car kit's tractor |
| Rig: aeroplane | fuselage; wings, stabilisers and fin fixed; ailerons, flaps, elevators, rudder; a three-bladed propeller with pitch; landing gear that folds, with wheels | Rig Proxy |
| Rig: helicopter | cabin, boom, skids; a four-bladed main rotor and a two-bladed tail rotor with pitch | Rig Proxy |
| Rig: tank | hull, a turret turning without end, a gun within its elevation, five road wheels a side, two treads of 64 links | Rig Proxy |
| Rig: gears and a piston | three meshing gears on a frame, a piston | Rig Proxy |
| Rig: robot arm | base, turntable, shoulder, elbow, wrist, flange, two-finger gripper | Rig Proxy |
| Rig: quadruped | spine, neck, head and tail chains, four animal legs | Rig Proxy |
| Rig: tentacle | a chain of sixteen joints | Rig Proxy |

The DeLorean and the tractor open with their stage's prims closed, drawn as
boxes: tick Show all geometry in the Scene Explorer, or open them.

What was checked: a test drives a car of two steering wheels, two meshing gears
and a tread: both wheels' tops go forward while driving, both wheels steer the
same way, the second gear turns exactly the other way, and every tread link
stays on its loop, checked every seventh frame, as the hull carries it. Rigid Bind and Rigid
Skin are tested on two pieces (one by a row, one by its nearest bone) moved
whole by their placement, their meshes shared; Rig Proxy gives one shape per
deforming joint, bound to it. Every template's operators were cooked and the
viewed ones looked at in the Viewer, the pictures above. The machines' motions
are what their joints allow, not driven: a gear's ratio and a tread's run are
the range of motion's, and a control rig would drive them the same way.

## Not done yet

- Components and templates still to make: a bird's wing, a spider's or an
  insect's legs (chains already serve), a bicycle (gears driving a wheel), a
  crane or an excavator (hinges and pistons, the piston aimed as the arm moves),
  doors that open (a gull-wing door on the DeLorean needs the door as its own
  primitive, which the model does not have)
- Driven relations outside the range of motion: a piston aimed at a moving
  part, a gear driving a wheel, a tread following its wheels on any clip
- Control rigs on the deform skeleton (the guides keep what they need: roles,
  poles, heels, ends, limits)
- Dragging locators in the Viewer; mirroring guides from one side to the other
- A clip held within the limits; the rotation order written to FBX
- Autofit tried on other bodies, on a T pose, and on a mesh with a head
