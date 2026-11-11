# USD

Load USD and Write USD are in the "File" sub-menu of the add-node menu. Pick,
Prune, Set Attribute and Unpack are in "Primitives".

## Nodes

| Node | What it does |
|---|---|
| Load USD | Reads a `.usda`, `.usdc`, `.usdz` or Alembic `.abc` file, composed. Gives one primitive per mesh, curves or points prim. Its Mode: sublayered at the file's paths, or the file's default prim referenced or payloaded at a Primitive Location |
| Cube, Sphere, Grid | A shape, as one primitive at its Primitive Location |
| Load VDB | An OpenVDB file, or a sequence, as a USD `Volume` at its Primitive Location: a field per grid (see [Volumes](#volumes)) |
| Merge | Any number of inputs as one: primitives side by side, or one mesh (its Mode) |
| Pick Primitives | Marks the primitives whose path matches a pattern as picked |
| Prune Primitives | Removes the primitives its Prims Location tab lists, or keeps only those |
| Set Attribute | Sets one value per prim: a constant primvar (Object) or the prim's own attribute or metadata (Property): `visibility`, `purpose`, `active`, `kind`, `instanceable`, `subdivisionScheme`, `doubleSided`, or any name |
| Edit Primitive | Edits the prims its Prims Location tab names as what they are (Xform, Mesh, Curves, Points, Camera, Light, RenderSettings): its form shows their values, and what is changed overrides them |
| Reparent | Moves prims under another prim, its Parent, with what is below them: those its Prims Location tab names, or the whole scene |
| Rename | Renames the prims its Prims Location tab names: Find replaced, a Prefix and Suffix added |
| Duplicate | Copies the prims its Prims Location tab names beside them, each a step further |
| Delete Attribute, Rename Attribute | Take attributes (primvars) away from the primitives' geometry, or rename them |
| Variant | Chooses variants of the scene's variant sets (Select): a stage is composed again with them, what was done above carried over. Makes a variant set on a prim, a variant per input (Create) |
| Unpack | Merges every primitive into one mesh |
| Write USD | Writes what the scene graph changed in its stages as a USD override layer |

## Composed stages

A file is read as a stage, with its composition resolved: sublayers, references,
payloads, inherits, specializes and variants, in text and binary layers alike.
What you see is what usdview would show.

Every prim keeps what the file says about it. Geometry stays in its own space,
as authored, and each primitive is placed by its world transform. Nothing is
baked into the points, so what is written back matches the file.

| | |
|---|---|
| Formats | Text (`.usda`), binary (`.usdc`), `.usdz`, and Alembic (`.abc`) read as a layer ([Alembic](#alembic)) |
| Composition | Sublayers, references, payloads, inherits, specializes, variants |
| Transforms | The whole `xformOpOrder`, `!resetXformStack!` included |
| Units and axes | Imago works in Y-up metres. A stage is read as authored by default: a Z-up stage in centimetres lies on its side, a hundred times too big, and its Load USD says so. With the operator's **Y-up metres** on, `upAxis` and `metersPerUnit` become a correction on the placement (the points stay as authored); turning it on or off reads the stage again. Either way a layer written has it where Imago shows it, in the [scene's](#the-scenes-axes-unit-and-rate) axes and unit |
| Meshes | Points, polygons of any size, left-handed orientation, subdivision scheme |
| Curves | `BasisCurves`: control points, counts, basis (linear, Bézier, B-spline, Catmull-Rom) and wrap, kept as authored |
| Points | `Points`, with their widths |
| Primvars | Every numeric primvar, normals and widths included, with its interpolation and indices |
| Instancing | Native instances and point instancers: each prototype's geometry is held once and shared by every copy |
| Purpose | `render`, `proxy` and `guide`, inherited down the hierarchy. Which prim authors it is kept, so a purpose set nearer with Set Attribute wins over one a mesh only inherits |
| Other authored values | `kind` and `instanceable`, and every attribute a prim authors with one value (`doubleSided`, `subdivisionScheme`, a light's `inputs:intensity`, `userProperties:*`…), listed in the Primitive Inspector's Properties tab as coming from the file |
| Materials | Listed in the node's Parameters: colour, roughness, metallic, opacity, and the texture behind each input. UsdPreviewSurface, OpenPBR and Standard Surface. A material whose shaders are MaterialX or UsdPreviewSurface ones has **→ MaterialX**: a MaterialX operator with its network, to edit and bind in its place (see [MaterialX](materialx.md#from-a-usd-stage)) |
| Cameras | Primitives with no geometry: projection, focal length, aperture and offsets, clipping range, f-stop, focus distance, where they are. Moved by Transform, looked through in the Viewer |
| Lights | Primitives with no geometry: every UsdLux type (Dome, Rect, Sphere, Disk, Cylinder, Distant), with its values (intensity, exposure, colour, temperature, size, texture, shaping), where it is and which way it faces. Moved by Transform, listed in the Primitive Inspector (see [Lights](#lights)) |
| Skeletons | Listed by name |
| Time samples | Every sample of the stage, over its time range, at the scene's frames: at the scene's rate a time code is a frame; at another, the stage is read at the same second, as USD retimes it ([below](#the-scenes-axes-unit-and-rate)). The timeline plays the file's animation (transforms, visibility, points and primvars, point instancers, cameras' lenses, lights' values) through the whole graph. A stage that changes over time cooks again at each frame, its still geometry shared; one that does not costs nothing more. When only transforms and geometry move (no point instancers), a frame is the stage as first read with the moving prims made again; anything else, and the whole stage is walked |

A file is read once and kept until it changes on disk. A `.usdz` is unpacked
beside itself so its textures are files the Viewer can read.

## Big stages: boxes first

A stage is read on a thread of its own, in two passes over one open stage, so
the program stays in use while it loads:

1. **Boxes.** The hierarchy, transforms, visibility, purpose and instancing, and
   each mesh, curves or points as the box of its authored `extent` (worked out
   from its points when it has none, so every prim with geometry has a box). The
   program waits for this pass, as it did for a whole load: there is nothing to
   show or work on yet. When it is done the Viewer and the Scene Explorer show
   the stage and the program is in use again; Transform, Prune and Set Attribute
   already work on it.
2. **The whole stage.** Every mesh array, primvar, material, binding and
   authored value, the geometry made on every core. What the boxes pass read is
   taken as it is, not read again: every prim's transform, visibility and
   purpose, and the points it read for a box where a prim authors no extent
   (when they do not change over time). When it is done it replaces the boxes,
   and everything downstream is cooked again.

Until the second pass is done, the Load USD node's parameters say *Shown as
boxes; reading the geometry…*, an opened prim shows its box, the Primitive
Inspector lists the boxes, and Write USD waits (it would otherwise write boxes
in place of geometry). On Moana the boxes take a fraction of the time the whole
island takes; `XMS_TIMINGS=1` prints both.

## Sublayer, Reference, Payload

Load USD's **Mode** says how the file comes into the scene, as in USD:

| Mode | In Imago | Written by Write USD |
|---|---|---|
| Sublayer | The whole stage, at the file's own paths | The file in the layer's `subLayers` |
| Reference | One prim of the file, with everything below it, at the **Primitive Location** (`/root` by default): `/Asset/geo/body` is `/root/geo/body`. The prim is the file's `defaultPrim`, or the one named in **Prim** (Pick lists the root prims). A file naming no default prim gives its first root prim, and the panel says so | `def "root" (prepend references = @file@</Asset>)`, with the edits under it as `over`s |
| Payload | As Reference: Imago always loads it | `prepend payload = @file@</Asset>`: other programs load it on demand |

- The prims above the Primitive Location are Xforms, made by Imago (`/shot/set`
  puts `shot` above). What the file has outside the referenced prim is left out,
  as USD leaves it out.
- Bindings and point-instancer prototypes inside the referenced prim follow it
  to its new paths. A binding to a material outside it keeps its path.
- A file's axes and unit are not converted by a reference in USD. Write USD
  puts the transform from where Imago shows it to the scene's axes and unit
  before the referenced prim's own (see below): opened, the layer gives what
  Imago shows.
- The Scene Explorer lists the stage at its new paths; Set Attribute, Material
  Binding and every Prims Location tab name it by them.

**Population Mask**, as USD's: rows of prim paths (typed, or dropped from the
Scene Explorer). Only the prims at those paths, what is below them, and the
prims above them (as their parents, without their other children) are read;
the rest of the stage is left out of Imago, so a big stage is read quicker.
In any mode: with Reference, the paths are the file's. Write USD leaves what
was not read as the file has it (nothing is deactivated). Empty, the whole
stage. Changing it reads the stage again.

The Parameters of a Load USD node show the stage: primitive and triangle counts
(every instanced copy counted), up axis and unit (by name: centimetres, inches…,
and whether kept as authored or turned to Y-up metres, with a note when a stage
is not Y-up metres, so a mismatch shows before a render does), a count of prims
by type, the materials with their textures, the cameras, skeletons and lights,
and a "Not read in full" list for whatever the file holds that is not handled
yet.

## Primitives

A stage comes in as primitives: each mesh, curves or points prim stays a
separate piece, with its path, material binding and purpose, and is passed from
node to node without being copied. Nothing is merged until a node needs one
mesh.

Transform, Edit Poly and Prune Primitives say which pieces they work on in their
**Prims Location** tab: a list of paths, a path taking its prim and everything
below it (see [Interface](interface.md#parameters)). An empty list means every
piece. Pick Primitives still chooses pieces for the operators after it with an
empty list, as before. Either way, on the chosen pieces:

- Transform moves each picked primitive on its own, by its placement. The
  geometry is not copied.
- Edit Poly, UV Unwrap, UV Transform and UV Edit on one picked primitive edit it
  where it is drawn, and it stays itself: same path, same material, same
  placement. Only what the node changed is replaced: a UV node leaves the points
  shared with the file.
- Several picked primitives go through Edit Poly and the UV nodes merged, as one
  primitive in the place of the first.
- Every other primitive passes through untouched.

Without a pick, Transform moves each primitive, and Edit Poly and the UV nodes
edit a lone primitive where it is drawn, keeping it itself. Several primitives
with none picked go through those nodes merged into one mesh.

An ICE node runs its graph on each primitive (or each picked one) in the
primitive's own space, as an ICE tree runs on an object in Softimage.

## Set Attribute

Set Attribute is the scene graph's one operator for setting values on prims. It
holds rows; each row sets one value:

| Context | Written as | For |
|---|---|---|
| **Object** | `primvars:name`, `interpolation = "constant"` | Data for rendering: shaders read it by name, and the prims below a group inherit it. `displayColor`, `displayOpacity`, a shader variation |
| **Property** | The prim's own attribute or metadata, by name | What the prim is and how it is used: `visibility`, `purpose`, `active`, `kind`, `instanceable`, `subdivisionScheme`, `doubleSided`, a camera's `focalLength`, `userProperties:*` |

Each row has a context, a name, a type and a value. **Pick** next to the name
offers the common names, each with its type and, where USD fixes them, its
values (`visibility` is `inherited` or `invisible`). Any other name can be
typed, with its type chosen: `bool`, `int`, `float`, `double`, `string`,
`token`, `float2`, `float3`, `float4`, `color3f`, `asset`, and the arrays
`int[]`, `float[]`, `float3[]`, `color3f[]`, `string[]`, `token[]`, typed with a
comma between elements (`1, 2, 3`; `(1, 0, 0), (0, 1, 0)`; `a, b`). An Object
value takes the types a primvar column holds (no `double`, `string`, `asset`
or `string[]`); an Object array is one list on the primitive, as ICE stores a
list per element. Where two rows set the same name, the later one wins.

**Keys.** ◇ beside a row keys its value at the playhead (◆ on a key, red when
the row has keys): keyed, the row has a value a frame, shown and changed at the
playhead (a change keys it there). Between two keys, floats, vectors, colours
and float arrays of one length go straight from one to the other; ints,
booleans, text and the rest hold the earlier key's value, as USD holds them.
Write USD writes a keyed row as `timeSamples` over its frames. ◆ takes the key
at the playhead away; the last one gone, the row keeps its value.

The Prims Location tab says where. Here a path **names** that prim alone, of any
type: a group, a camera or a light as well as a mesh, so `kind` on `/DeLorean`
is one opinion on `/DeLorean`, not one on each of its meshes. `/set/*` names the
children of `/set`, `/set/**` `/set` and everything below it. With no location,
the rows are set on every primitive coming in (or the picked ones).

Any prim can be named: a mesh, a group or camera of a loaded stage, or one of
the Xforms Imago makes above a shape's Primitive Location (`/root/asset/geo`
above `/root/asset/geo/cube`).

In Imago:

- An Object value on a primitive is an Object attribute of it: the Primitive
  Inspector's Object tab shows it, ICE reads it.
- `purpose` is the primitive's purpose: the Scene Explorer's purpose toggles
  show or hide it.
- `active` off takes the prim and everything below it out of the scene.
- `visibility` invisible hides the prim and everything below it in the Viewer.
- The rest is kept on the prim and written by Write USD.

What is set on a group is inherited by the primitives below it, as USD does:
`visibility` invisible and `active` off reach everything below; `purpose` comes
from the nearest prim that has one, the primitive itself first, whether the file
authors it there or Set Attribute set it there (on the same prim, Set
Attribute's wins); `kind`, `doubleSided` and the other Properties are not
inherited; an Object value reaches each primitive below that has no attribute of
that name, the nearest group winning. The Primitive Inspector's Object tab lists
inherited values with the primitive's own; its Properties tab lists a prim's
properties and says, for each, whether it comes from the file, from Imago, from
which Set Attribute operator, or from which group above it. ICE reads only a
primitive's own attributes. Write USD writes each value once, on the group.

Values per point, per face or per corner are not set here: changing them
rewrites the primitive's arrays, and that is ICE's.

## Edit Primitive

Edit Primitive edits prims already in the scene, a loaded stage's above all, as
what they are. The quickest way to one: select prims in the Scene Explorer, or a
row of the Primitive Inspector's Properties tab, and press **Enter** with the
pointer over it, or right-click and choose **Edit Selected**. An Edit Primitive
naming them is added after the viewed operator, viewed and selected, its form
in Parameters; asked again while it overrides nothing, the same one names the
new selection. Name them in its Prims Location tab (a path names that prim, as
Set Attribute's do); its **Type** sets itself from the first one: Xform (a
group, or any prim), Mesh, Curves, Points, Camera, Light or RenderSettings. Its form is that
type's, with the values of the first prim of that type named.

| Type | Its form |
|---|---|
| Every type | **Xform**: Translate, Rotate, Scale (the prim's own transform, relative to its parent), Visibility, Purpose, Active, Kind |
| Mesh | Double sided, Subdivision |
| Curves | Type (linear or cubic), Basis (bezier, bspline, catmullRom: how cubic curves pass through their points), Wrap (nonperiodic, periodic, pinned), as USD names them. Written with the curves' counts, as their topology |
| Camera | Projection, focal length, filmback and its offset, clipping range, f-stop, focus distance, as the Camera operator has them |
| Light | Its UsdLux values, as the Light operator has them |
| RenderSettings | Render Commons' values by their USD names: camera, resolution, pixel aspect, aspect conform, data window, motion blur and depth of field off. No Xform section. A file's prim shows what it authors (USD's fallbacks elsewhere; its camera is not read yet) and is written as an `over` of what is overridden: it stays the one rendered when it was |

**Only what you change is an override.** A value changed in the form is marked
✎, with ↺ beside it to give it back to the prim; the **Overrides** group lists
them, with **Revert all**. Everything else follows the prim: a value the file
changes, or animates, still comes through. Overrides can be keyed, and only
they can (the Keys group lists them). Overrides are absolute: a Translate
overridden puts the prim there, under its parent. To move prims by an amount,
use Transform.

Every prim named of the Type takes the overrides; one of another type is left
as it is, and the panel says how many. The Xform section's values reach every
prim named of the type. A group named moves what is below it.

Write USD writes the overrides over the stage: a file's camera's or light's
values as its own attributes (`float focalLength = 85`), visibility, purpose,
active and kind as Set Attribute's are, a moved prim as its own transform. A
camera or light made in Imago is written whole, with the overrides in it. A
group of a file moved is written as the group's own transform, the file's
hierarchy kept: the prims below it get no transform of their own unless they
moved too. (A group Imago made above its shapes is written as its shapes' new
places.)

With an Edit Primitive selected, the Viewer's handles sit on the first prim it
names of its Type: W moves, E rotates, R scales (not a camera), working in the
prim's parents' space, and a drag overrides that part alone (Translate, Rotate
or Scale), as typing it in the form does.

Only Edit Primitive has overrides like these: every other operator's parameters
are simply its values.

## Reparent

Reparent moves prims under another prim, each with what is below it. Its
**Parent Location** is the Parent: a prim of the scene (Pick lists them) or
a new path, made as an Xform (with Xforms above it). **Move** says what goes
there: **Primitives**, the prims its Prims Location tab names (a path names
that prim, as Edit Primitive's do; a prim below another named goes with it),
or **Whole scene**, every root prim. A prim keeps its name, unless the Parent
already has a child of that name: then a number is added (`set1`), and the
panel says so. A prim the Parent is in, or is, stays where it is.

With **Keep World Position** on (the default), what moves stays where it was in
the world: the prim's own transform changes to make up for its new parent's.
Off, it keeps its own transform and follows the Parent.

What comes after sees the new paths. Write USD writes a file's prim moved as a
`def` at its new path referencing the file's prim (`prepend references =
@set.usda@</set>`), its new transform there when it has one, and deactivates
it at its old path; edits below it are written at their new paths. A material
binding from inside it to a material outside it is written again on the prim,
since a reference drops targets outside the prim it brings in; other
relationships to prims outside it (a light's linking, a skeleton's binding)
are not carried yet. A prim Imago made is written whole where it now is.

## Rename and Duplicate

**Rename** renames the prims its Prims Location tab names (`/set/*_old`):
**Find** is replaced by **Replace** wherever it is in the name, then **Prefix**
and **Suffix** are added. What is left is made a name USD takes (`lamp.hi` is
`lamp_hi`); a name a sibling already has gets a number. Each prim keeps what is
below it and its place in the world, and is written as Reparent writes a move.

**Duplicate** copies the prims its Prims Location tab names, each with what is
below it, beside it under the same parent. **Copies** says how many; **Name**
names each, `{name}` the prim's and `{n}` the copy's number (`{name}_{n}`:
`crate_1`, `crate_2`); a name taken gets a number. Each copy is moved from the
one before by **Translate**, **Rotate** (degrees, X then Y then Z) and
**Scale**, in the parent's space, so 3 copies translated 2 in X sit at 2, 4 and
6. A prim below another named goes with it. Write USD writes a file's prim
copied as a `def` referencing it, with the copy's transform; the original stays
as it is.

## Delete Attribute and Rename Attribute

**Delete Attribute** takes attributes (primvars) away from the geometry of the
primitives its Prims Location tab names, or every one. **Names** are words, by
spaces or commas: `*` is any run of characters, `?` any one, and a word after
`^` keeps what it names (`* ^st` takes all but `st`). A word is a whole name:
`uv` is not `uv1`. **Context** limits it to Point, Face, Corner or Object
attributes. The points are never taken.

**Rename Attribute** renames each word of **From** to the word in its place
in **To** (`Cd uv` to `displayColor st`). A `*` in both keeps what it matched:
`uv*` to `st*` makes `uv1` `st1`. An attribute already of the new name is
replaced, in any context, since USD has one attribute of a name.

The panels list the attributes coming in and what changes, with how many
primitives have each. Write USD writes an attribute taken away as blocked
(`primvars:Cd = None`), so the file's value is not seen, and a rename as the
new name written and the old one blocked.

## Variants

A stage's variant sets are listed as it loads, names only: the variants not
selected are not read. A prim with variant sets says `variants` in the Scene
Explorer, and its Properties tab has a `variant:<set>` row, the variant
selected first, then every one it has.

**Variant**, in Select, chooses: each of its choices is a prim, one of its
variant sets and a variant, picked from what the stages coming in have. Right-
click a prim in the Scene Explorer, **Variant Select ▸ set ▸ variant**, to add
one after the viewed operator (the viewed Variant takes it, when it is one).

The stage is composed again with the choices in a session layer, kept by
choice, so going back is instant. Of the prims coming in:

| Prim | After the choice |
|---|---|
| Outside the prim whose variant changed | As it came, with everything done above |
| Inside it, left as it was by the variant | As it came, with everything done above |
| Inside it, changed by the variant | The new one, with its placement, material, purpose, Set Attribute and Edit Primitive values carried over; what reshaped the old geometry (Edit Poly, UV, ICE) is not, and the panel says so |
| Brought by the variant | Added, unless its path was pruned above |
| Not in the variant chosen | Gone; the panel says when something was done to it |

Write USD writes the choices as `variants` on an `over` of each prim, and the
layer composes as Imago showed it. Put a Variant before a Reparent of the same
stage: after one, it says so and leaves the stage as it is.

**Variant**, in Create, makes a variant set: its **Primitive Location** is the
prim it is on (a prim of the scene, or a new path, made as an Xform), its
**Variant Set** its name. Each input after the scene is a variant, named in the
panel: what that input has below the location, made in Imago (shapes, meshes
made by operators) or a stage: a Load USD in Reference or Payload mode, its
Primitive Location below the Variant's (`/asset/car` for `/asset`). A stage
goes in whole, at its own paths, so it is one in the Scene Explorer: its own
variant sets can be chosen too. The scene goes on with the one
**Shown**; what it had below the location gives way to it. Select and the Scene
Explorer's Variant Select choose among the variants as they do a stage's.

Write USD writes the set on its prim, `variantSets`, the selection and every
variant, so the file switches as Imago does: inside each variant, its prims
made in Imago as `def`s, its stages as their `references` or `payload`, with
what was done to them as `over`s. The shown variant is written as it is at the
end of the graph, edits after the Variant included; the others as they came
in. A sublayered stage cannot go in a variant (its paths are the file's), and a
variant with nothing below the location makes no prims: shown, nothing carries
the set, so Write USD does not write it.

The templates **Variant set: cube or sphere** and **Variant set: teapot or
vehicle** (Martino ▸ USD stages) make `/asset`'s set from a cube and a sphere,
and from the teapot and the car kit's vehicle.

## Shapes are primitives

Everything in the scene is a primitive, so Cube, Sphere and Grid each make one,
at their **Primitive Location**: `/cube`, `/sphere` or `/grid`, or any path
typed there, such as `/root/asset/cube1`. The parents on the path are Xforms, as
in Solaris. What is typed is made a valid USD path: empty segments are dropped,
and characters a USD name cannot hold become `_` ("Used as …" under the field
says so).

Merge joins its inputs, in order, by its **Mode**. It takes any number: wiring
its last input adds another, so one is always free (Inputs, in its Parameters,
sets how many). One Merge does what a chain of them did.

| Mode | What it does |
|---|---|
| Auto | Goes by the first input: primitives are merged as primitives, a plain mesh or a clip is flattened |
| Primitives | The primitives of every input side by side, each keeping its path. A plain mesh among them becomes a primitive at `/mesh` |
| Flatten | Everything as one plain mesh, as before primitives were merged, and as Unpack does |

On a path two inputs have, the later input is the stronger, as an `over` on the
same path in USD: its primitive takes the first one's place. What it does not
say is read from the first one: a shape put on the path of a prim loaded from a
file keeps that prim's material and stage, and is written as a change to it.

Stages are as strong as their inputs, whichever order they come in: a layer
written sublayers the later input's stage first (USD's strongest), and a prim
of one stage that another stage's prim took the place of is left under it, not
deactivated. USD composes the two on that path; Imago shows the stronger
one's alone, and Write USD's panel and the render's Log say so.

The pattern is a name pattern as described in
[Interface](interface.md#name-patterns): comma-separated regular expressions
matched against the full prim path, with a "Pick" list of the paths coming in.

![One wheel picked out of a 461,595 triangle model and moved](xms_usd_pick.png)

## Instancing

Native instances and point instancers keep their instancing. The geometry of a
prototype is held once and each copy is a placement: a native instance gives one
primitive per mesh, under the instance's own path; a point instancer gives one
primitive per prototype mesh, holding all of its points.

- Transform moves the placements, not the points, so moving a forest is moving
  matrices.
- A node that changes geometry (Edit Poly, the UV nodes) on one instance makes
  that instance a mesh of its own. The other copies stay shared.
- The Viewer draws a mesh with many copies in one draw call (GPU instancing), in
  its material's colour. A mesh with 64 copies or fewer is drawn copy by copy
  with its full material and textures.

A point instancer is also a primitive of its own, at its path: a points
primitive with one point per instance and the instancer's arrays as Point
attributes (`points` from `positions`, `orient`, `scale`, `protoIndex`, `id`),
in its own space. It is not drawn (its prototypes are, at its points) and not
merged into a mesh. Selecting it in the Scene Explorer lists its instances in
the Primitive Inspector's Point tab, one row per instance, and nothing else. A
group above it, or the whole scene, lists it the same way: its prototypes'
geometry is listed only when a prototype (a row below the instancer) is
selected; its Properties tab gives the number of instances and its prototypes.

Its points are where the copies are: whatever changes them (an ICE graph moving
`P`, a Transform on the instancer alone, through its Prims Location tab or a
Pick) moves the copies, which are made again from the points after every
operator, only when the instancer changed. Write USD writes what changed as the
instancer's own arrays (`positions`, `orientations`, `scales`, `protoIndices`,
`ids`), and an instancer moved as a whole gets its own transform; its
prototypes' copies are not edits of their own.

A prototype is listed where the stage's hierarchy has it, under the instance it
is seen through when the instancer is inside a native instance (the corals of
Moana's island), so the Scene Explorer lists each prim once. Should a drawn prim
still not be where the hierarchy lists it, the node's "Not read in full" says
how many and gives the first.

**Nested point instancers**, a point instancer inside a prototype of another
(grass tufts scattered by a field instancer, each tuft its own instancer of
blades), are drawn through the outer one, as USD composes them, down to four
deep. The inner instancer is a points primitive of its own, placed once per
point of the outer instancer using its prototype; its prototypes' meshes are
drawn at each of its points in every one of those copies. Changing the outer
instancer's points moves the inner instancer's copies and, a level down, what
they draw. Write USD lists edits of an inner instancer as not written, as for
any prototype.

The Primitive Inspector lists instanced geometry once, in its own space, not
once per copy: a point instancer's prototypes, and the meshes of native
instances. Their Properties tab gives the number of copies. What it lists grows
with the geometry there is, not with what is drawn, so a stage of millions of
instanced points opens in it at once.

## The Scene Explorer and the Viewer

The Scene Explorer lists a stage as its hierarchy: groups, meshes, curves,
points, materials, cameras, lights and skeletons, each with its icon. Pruned
primitives leave the list, and so do groups left empty.

What is drawn follows the explorer:

- A closed prim is drawn as the box around everything below it. The island first
  shows as one box per root prim.
- An open prim hands the decision to its children.
- A mesh, curves or points prim whose parent is open is drawn as geometry.
- The square at the right of a row draws that prim and everything below it as
  geometry, however far it is opened.
- "Show all geometry" in the explorer's header draws everything.

Opening and closing does not cook the graph again. The boxes come from the
primitives as they reach the Viewer, so they follow a Transform or a Prune
upstream. A point instancer's box covers every copy, and an open instancer shows
one box per prototype.

The **guide**, **proxy** and **render** toggles in the explorer's header choose
which purposes are drawn. By default guides are hidden, proxies are shown, and
render geometry is shown where its asset has no proxy (where it has one, the
proxy stands in for it). A prim with no purpose is always drawn.

Curves are drawn as lines through their basis, points as small crosses sized by
their width.

**Draw modes.** A prim whose `model:drawMode` says so is drawn as its bounding
box (`bounds`; `cards` too) or as axes at its origin (`origin`), at every copy,
as USD viewers draw it; a point instancer's prototype is drawn as one box (or
one origin) per instance, around all its prims; it applies below it too, the
nearest prim with one deciding. Set Attribute offers `model:drawMode` and
`model:applyDrawMode`; an ICE Instancer's Draw Mode sets them on its prototypes.
A right-click on a row of the Scene Explorer chooses for the Viewer alone, over
what the scene says, and is not written. Past 20 million triangles a frame the
heaviest prims are drawn as boxes (see
[Interface](interface.md#the-viewers-triangle-budget)).

## Materials in the Viewer

Primitives are drawn with their materials: colour, roughness, metallic, opacity,
the colour texture and the emissive texture. Textures packed in a `.usdz` are
used from where it was unpacked.

![The example model with its textures](xms_usd_textured.png)

- Texture files are decoded in the background. A surface shows its plain colour
  until its texture is ready, and "loading N" shows next to the Viewer menu
  meanwhile.
- PNG, JPEG, TIFF, TGA, HDR and OpenEXR are read. A float image (EXR, HDR) holds
  linear light and is shown as such.
- **UDIM tiles:** a texture path with `<UDIM>` (or `<udim>`) in place of the
  tile number (`body.<UDIM>.exr`) reads every tile there is, from 1001 to 1999,
  as Mari, Substance and USD name them. The tiles are laid out as they lie in UV
  space and each part of the mesh shows the tile its UVs fall in; a tile not
  there is grey. The tiles share 4096 pixels across the widest way (1,024 each
  for up to four, 409 for ten). It works for a USD stage's textures, the
  Material operator's and a MaterialX graph's alike. In the file browser, with
  **Collapse sequences** and **UDIM** on, the tiles are one entry,
  `body.<UDIM>.exr`, which picking gives as it is.
- Images larger than 2048 pixels on a side are reduced to that. Each gets its
  chain of smaller copies, so distant surfaces do not shimmer. Textures repeat
  outside the unit square.
- A material with opacity below one is blended. A material whose opacity comes
  from its colour texture is cut out by that texture's alpha.
- "Textures" in the Viewer menu turns all of this off and shows plain grey.

## Making materials

The **Material** operator (Add menu, Materials) makes a UsdPreviewSurface
material. Its parameters are the surface's inputs: diffuse and emissive colour,
metallic, roughness, opacity, opacity threshold, IOR, clearcoat and its
roughness, with UsdPreviewSurface's defaults. A Textures group takes an image
file for diffuse, emissive, metallic, roughness, opacity, normal and occlusion;
an input fed by a texture is greyed out, its value coming from the texture. An
opacity file that is the diffuse file reads that texture's alpha.

The material is named by its operator: rename the node to rename it (spaces and
other characters USD does not take become `_`). Viewed on its own, it is shown
on a sphere, with its colour, opacity and textures in Textured mode, and the
Scene Explorer lists it as one row.

It puts out a material, not a scene: Material Binding puts it in the scene and
binds it. In USD it is a `Material` holding a `UsdPreviewSurface` shader, a
`UsdUVTexture` per file (colours read as sRGB, the rest raw, normal maps scaled
to -1..1) and a `UsdPrimvarReader_float2` reading `st`.

## Binding materials

**Material Binding** (Add menu, Materials) takes the scene on its top input and
materials on its left side, one per input, numbered. A Material's output is on
its right side; sockets carrying a material are purple. Wiring the last material
input adds another, so one is always free; Inputs, in its Parameters, sets how
many.

- **Material Library Location** (`/materials` by default): every material wired
  in goes there, by its operator's name (`/materials/CarPaint`), as a `Material`
  prim with no geometry. Its parents are Scopes. A material already on that path
  is replaced. Two inputs whose materials have one name: the first is used, and
  the Parameters say so.
- **Rows** are on its **Bindings** tab (the Prims Location tab of the other
  operators): each row takes the primitive at its path and every one below it,
  as on Transform's tab, and binds its material to them. A row's Material column
  says which: the input of the row's number unless another input with a material
  wired in is chosen, or **Location**: a material already in the scene (read
  from a file, or put there by an earlier Material Binding), by its path.
  Location opens a line under the row for that path: type it, pick it from the
  materials of the scene coming in, or drop the material from the Scene
  Explorer. On a primitive two rows take, the later row wins. No rows: nothing
  is bound.
- Beside the Material column, each row's **purpose**: All
  (`material:binding`, for every render), Preview
  (`material:binding:preview`, for viewports: the Viewer draws it) or Full
  (`material:binding:full`, for final renders; Arnold reads it from the layer
  Imago writes). The later row wins for each purpose apart. And **⬇**, its
  strength: lit, the binding is stronger than those of the prims below the
  ones it binds (`bindMaterialAs = "strongerThanDescendants"`); off, weaker,
  USD's default. Files' preview and full bindings and strengths are not read
  yet: the Viewer draws a file's binding for all.
- A row on a group binds each primitive below it, written on each one: what the
  file bound below it is replaced. A point instancer is not bound itself; its
  prototypes, below it, are.
- In the Viewer, Textured mode shows each primitive in its new material. A
  material of the file that no primitive wore before is shown grey for now.
- The Scene Explorer lists the materials under the library, of type Material.
  Selected, the Primitive Inspector's Properties tab lists the surface's inputs,
  from the Material operator that made it; a bound primitive's
  `material:binding` row comes from the Material Binding that bound it, and Go
  to Operator selects either.
- Write USD writes the materials as `def Material`s with their shaders, under
  `def Scope` parents, and each binding as `rel material:binding` (with
  `MaterialBindingAPI`) on the primitive.

## Cameras

A camera is a primitive with no geometry: its lens and gate as USD has them, and
its placement, looking down its -Z axis with +Y up. Load USD gives a file's
cameras; the **Camera** operator (Add menu, Create) makes one at its Primitive
Location (`/camera` by default), with Translate and Rotate, Projection, Focal
length, Filmback (the gate; Filmback Presets: USD default, full frame, Super 35,
ALEXA Mini 16:9, ALEXA Mini Open Gate, ALEXA Mini LF Open Gate), Filmback
offset, Clipping, f-stop and Focus distance. Focal length and gate are in tenths
of a scene unit, as in USD: millimetres in a centimetre scene. The field of view
comes from their ratio and is shown under them.

- Transform moves a camera, as any primitive; Merge joins it to a scene; Set
  Attribute sets its attributes.
- The Viewer draws each camera as a pyramid from its eye to its gate, as long as
  its lens.
- **Looking through:** the Viewer's camera menu (🎥 Perspective, next to the
  display mode) lists the cameras of the scene; choose one to look through it,
  Perspective to go back to the Viewer's own camera as it was. A Camera
  operator's **Look through** does the same. The whole gate is shown, what lies
  outside it darkened, the camera's name at the top, in the middle. Orbiting,
  panning, zooming or framing leaves the camera: the Viewer's own takes its
  place, where it was, with its field of view, turning about its focus distance
  (or 5 m).
- The Primitive Inspector's Properties tab lists a camera's values; Go to
  Operator selects the Camera operator that made it.
- Write USD writes a camera made in Imago as `def Camera` with its transform and
  lens (`projection`, `focalLength`, `horizontalAperture`, `verticalAperture`
  and `clippingRange` always, the rest when set), and a file's camera that moved
  as its new transform. A pruned camera is deactivated.

## Lights

A light is a primitive with no geometry, as a camera is: its UsdLux type, its
values, and its placement, Rect, Disk and Distant lights emitting along their
-Z. Load USD gives a file's lights; the **Light** operator makes one.

**The Light operator** is one operator with a **Type**: Dome, Rect, Point,
Sphere, Disk, Cylinder, Distant (a point light is a sphere light with **Treat as
Point** on, as USD has it: UsdLux has no point light of its own). The add menu's
**Lights** category has an entry for each, the same operator set to that type:
Dome Light, Rect Light, Point Light… Changing its Type keeps the values both
types have, and its Primitive Location.

- **Primitive Location**: `/lights/$OPNAME`, under `/lights` and named after the
  operator (`/lights/DomeLight` for a new Dome Light), whatever its type.
  **Translate** and **Rotate**, as the Camera's, with the Viewer's handles. A
  new light is 2 m up and 4 m back, turned 30° down; a dome at the origin (where
  a dome is does not matter, its rotation turns the environment).
- **Light**: Intensity, Exposure (stops), Colour, Use Temperature and
  Temperature (kelvin), Normalize. **Contribution**: Diffuse and Specular
  multipliers. **Shape**: the type's own: a dome's Texture, Format (automatic,
  latlong, mirroredBall, angular, cubeMapVerticalCross), Pole Axis and Guide
  Radius (how big it is drawn: 1 on a new Light operator, UsdLux's fallback
  being 100000); a rect's Width, Height and Texture; a sphere's Radius and Treat
  as Point; a disk's Radius; a cylinder's Length, Radius and Treat as Line; a
  distant light's Angle.
- Each value starts at UsdLux's fallback (1 for intensity, 50000 for a distant
  light's, 6500 K, 0.53° for the sun's angle); one changed has a dot after its
  label and its fallback in its tooltip. **Reset to the schema's** puts them all
  back.

The parameters are laid out from `assets/usdlux.json`, written from OpenUSD's
schemas: LightAPI's attributes, then each type's, with their USD names, types,
fallbacks, groups and help. Write USD writes a light made in Imago as its type
(`def DomeLight "dome"`) with its transform and every value
(`float inputs:intensity = 1`, `uniform token poleAxis = "scene"`…), an empty
texture left out; a file's light that moved, as its new transform. The Primitive
Inspector's Properties tab lists a light's values; Go to Operator selects the
Light operator that made it.

**In the Viewer**, each light is drawn as its shape, in its colour (times its
temperature's when that is on, brought to full brightness so a dim light still
shows):

| Type | Drawn as |
|---|---|
| Dome | Half a sphere, as its name says: two arches and a ring at 45 degrees over a ground circle, with its pole, and the compass on the ground (N along -Z, with the longer tick, E along +X, S, W). At its guide radius, but at most 12 m (a file's dome at UsdLux's 1 km would be past the far plane); with Pole Axis Z, its top is along +Z. Its placement's scale is not a size |
| Rect | Its rectangle, width by height, with its diagonals and an arrow along -Z, the way it emits |
| Point | A small star |
| Sphere | Three circles at its radius |
| Disk | Its circle, and an arrow along -Z |
| Cylinder | A circle at each end, along X, and four lines between |
| Distant | A small circle and five parallel arrows along -Z |

Click a light's lines to select it, as a mesh is selected; selected, its lines
turn the selection's colour, and a Light operator selected has the Move and
Rotate handles. Cameras are picked by their pyramid the same way. A file field
(a dome's Texture, a rect's) has 📂 to browse for the image (EXR, HDR, PNG, JPEG,
TIFF, TGA) and ✕ to clear it.

Not yet: lighting the Viewer with them, a dome's image as its environment, a
file's light's values edited, ShapingAPI, ShadowAPI and portals in the operator.

## Volumes

**Load VDB** (File) puts an OpenVDB file, or a sequence (`smoke.$F4.vdb`,
`####`; picking one frame takes the sequence), in the scene as a USD
`Volume` at its Primitive Location (`/volumes/$OPNAME` by default). Imago
reads the file's header and each grid's metadata, not its voxels: the
grids' names, value types, classes (level set, fog volume), voxel sizes,
active voxel counts and bounds, as OpenVDB writes them with each grid. A
renderer reads the file itself.

- **Write USD** writes `def Volume` with its `extent`, its material binding,
  and a field per grid: `rel field:density` to an `OpenVDBAsset` below it,
  with `asset filePath` (the file of each frame, as time samples for a
  sequence) and `token fieldName`. A second grid of the same name is
  `density_1`, with its `fieldIndex`. Point data grids are listed but not
  written as fields.
- **The Viewer** draws its box, in light blue. Transform moves it. A file written without
  OpenVDB's statistics does not say its bounds: no box, no `extent`.
- **The Primitive Inspector**'s Properties tab lists its file and each grid.
- **Rendering:** Arnold's USD reader renders the `Volume` from the file. Its
  shading is a MaterialX operator's `ai_standard_volume` into a
  `volumematerial`, bound with Material Binding (see [Renderers'
  shaders](materialx.md#renderers-shaders)).

The file is read with [OpenVDB-rs](https://github.com/OxidePicturesAlliance/vdb),
a Rust port of OpenVDB, pinned to a commit.

Not yet: Volume prims of a loaded stage in the Viewer (they render, as part of
the stage, but are not drawn), the voxels themselves (a slice or points in the
Viewer, sampling in ICE, meshing), instance grids' bounds.

## Write USD

Write USD writes what the scene graph changed as a `.usda` override layer. The
layer sublayers the file each sublayered stage was read from, references or
payloads each stage loaded that way on its prim, and holds only opinions over
them, so opened on its own (in usdview, rray, or Imago) it gives what Imago
shows. Its stages are sublayered strongest first (the later input of a
Merge, [above](#shapes-are-primitives)). It says the scene's `upAxis`,
`metersPerUnit`, `timeCodesPerSecond` and `framesPerSecond` (below).

### The scene's axes, unit and rate

File > Settings > **Scene** sets the scene's up axis (Y or Z), unit (metres,
centimetres…) and frames a second, saved with the graph (the timeline's rate is
the same one). Imago itself always works in Y-up metres; these are what every
layer it writes says (Write USD's, a render's), so a show's convention is kept
whatever its files are in.

- **Axes and unit.** USD corrects neither across a sublayer or a reference:
  stage metadata is the root layer's alone, and a file of other axes composes
  as authored. So each stage is put where Imago shows it, in the scene's axes
  and unit, by a transform before its root prims' own:
  `matrix4d xformOp:transform:toScene`, first in their `xformOpOrder` (after
  `!resetXformStack!`), the file's ops after it, animated or not. A referenced
  stage has it on the prim the reference is on. A root prim two stages have is
  put as the stronger has it, and the panel says so when they differ.
- **Rate.** USD retimes a sublayered or referenced file whose
  `timeCodesPerSecond` is not the root layer's, keeping seconds: a 25 fps cache
  in a 24 fps scene has its time code 25 at frame 24. Imago reads stages the
  same way, so the Viewer and a render agree, and the Load USD panel, Write
  USD's and the render's Log say when a stage is not at the scene's rate. **Use
  as Scene Range** takes the stage's.

Set a path and press **Write**. The panel then says what was written, and lists
what could not be, with the reason.

| Changed in Imago | Written |
|---|---|
| A prim moved | Its own transform (`xformOp:transform`), so it still follows its parents |
| Points moved | `points`, and `extent` to match |
| Polygons or curves changed | `faceVertexCounts` and `faceVertexIndices`, `orientation`; for curves their counts, type, basis and wrap |
| Primvars written or added | `primvars:` with their USD type (`texCoord2f`, `color3f`, `normal3f`…), interpolation and indices; a list per element (ICE's stored lists) with its `elementSize` |
| Attributes lost | Blocked (`= None`), so the file's value is not seen through |
| Several prims merged into one | The whole geometry, in the prim's own space |
| A prim pruned | `active = false` |
| A stage loaded as Reference or Payload | `def "root"` (its Primitive Location) with `prepend references` or `prepend payload` to the file's prim, and the axis and unit transform where needed; its parents `def Xform` |
| Material binding or purpose changed | `material:binding`, `purpose` |
| A material Material Binding put in the scene | `def Material` with its `UsdPreviewSurface`, `UsdUVTexture` and `UsdPrimvarReader_float2` shaders, under `def Scope` parents |
| A point instancer's points or their attributes | `positions`, `orientations`, `scales`, `protoIndices`, `ids` |
| Set Attribute values | Object: `primvars:name` with `interpolation = "constant"`. Property: an attribute (`token visibility`, `uniform bool doubleSided`, `uniform token subdivisionScheme`, any other name with its type) or metadata (`active`, `kind`, `instanceable`). On any prim of the stage: a group, a camera, a light |
| A light made by the Light operator | `def DomeLight`, `def RectLight`… with `xformOp:transform` and every UsdLux value |
| A camera made by the Camera operator | `def Camera` with `xformOp:transform`, `projection`, `focalLength`, the apertures, `clippingRange`, and `fStop`, `focusDistance` and the offsets when set |
| Render prims the render operators made | `def RenderSettings`, `def RenderProduct`, `def RenderVar` under `def Scope` parents, with their attributes and relationships; the layer names the RenderSettings prim (`renderSettingsPrimPath`). One of a loaded file: an `over`, its relationships appended. See [Rendering](render.md) |
| A prim made in Imago (a shape, say) | A new prim (`def Mesh`, `def BasisCurves` or `def Points`), its parents `def Xform` where no stage has them |

The layer's `defaultPrim` is the first root prim of the first stage, or, with no
stage, the first prim made in Imago (`cube` for a lone Cube). Prims made in
Imago are written in the scene's axes and unit.

Only what changed is written: a stage nothing touched gives a layer with its
header alone. What changed is found by comparison with the stage as it was
loaded (see [Geometry](geometry.md)).

Not written yet, and listed by the panel:

- Changes to prims inside a native instance. USD keeps instance proxies
  read-only: the prototype has to be edited, or the instance made a prim of its
  own.
- Changes to a point instancer's prototypes, which go through the instancer's
  positions, orientations and scales.

**Frames.** Write USD writes the scene's range by default (or the current frame,
or a range and step of its own; a step of 0.5 writes samples between frames,
for motion blur). Each frame is cooked and compared with the stage at that
frame; what changes from frame to frame is written as `timeSamples`, what does
not as a single value, and the layer says `startTimeCode`, `endTimeCode`,
`timeCodesPerSecond` and `framesPerSecond`. A prim edited at any frame is
written at every frame, so the file's own animation is not seen through where
Imago left it alone. Imago's own animation (keys on a Transform, a camera, a
light, a shape) is written the same way. Nothing above Write USD changing over
time, one frame is written. Relationships and `uniform` values cannot be
animated in USD: when one changes, the first frame's is written, and the panel
says so.

The Load USD panel shows the stage's time range, and **Use as Scene Range**
makes it the scene's frames and rate.

**Flatten.** On, Write USD writes one file of the whole scene instead of an
override layer: the stages' files and Imago's changes composed into it, as
USD's flatten (`usdcat --flatten`): every prim with its values and time
samples, no sublayers, references or payloads, so it opens without the files it
came from. Named `.usda` or `.usdc`, it is written as that. The override layer
is made beside it first and taken away once flattened.

## Not done yet

- Load USD: a layer offset on a reference, several references on one prim,
  payloads left unloaded; a population mask's relationship targets (a point
  instancer's prototypes outside it) are not added to it
- Two stages with a prim on one path: Imago shows the stronger one's alone,
  where USD composes the weaker one's attributes under it
- Transforms in the Properties panel are Imago's, in Y-up metres, whatever
  the scene's axes and unit

- Write USD: editing inside instances; the stage's own `defaultPrim` (the
  first root prim is used); flattened, native instances are written as copies
  and value clips' samples are not baked in
- A Primitive Location for the operators that make a plain mesh (Unpack, Merge
  set to Flatten); until then it has no path, and Write USD says so. ICE keeps
  the primitives it is given, with their paths; a plain mesh going into it stays
  plain
- Normal, roughness, metallic and occlusion maps in the Viewer
- Cameras: gate offsets and depth of field in the Viewer, cameras inside native
  instances
- Animation: topology that changes over time (a mesh's polygons are those of
  the stage's start time), skeletal animation, skinning, blend shapes, value
  clips; a file's camera's apertures and clipping range, and lights' shapes, are
  read at the start time
- Per-face material subsets, creases and holes (kept in the file, not shown),
  NURBS, a stage's volumes, implicit shapes (sphere, cube, cylinder…)
- Point instancers nested more than four deep (listed in the node's "Not read in
  full")
- Materials with a graph inside (compounds); editing materials read from a
  file (Edit Primitive has no Material type yet)
- Material Binding: binding a group as a group (one `material:binding` on it,
  inherited below, with `bindMaterialAs` strength), binding purposes (`preview`,
  `full`), collections; the colour of a file's material no primitive wore before
- Set Attribute: inherited Object values read by ICE; arrays of `double`,
  `float2`, `float4`, `asset`
- Arrays, dictionaries (`customData`) and relationships other than the material
  binding are not listed in the Properties tab
- Write USD writes nothing for a stage none of whose primitives is left (all
  pruned, or all below a deactivated group)
- A point instancer's `invisibleIds` are kept (those instances are not drawn)
  but not listed; `velocities` and `accelerations` are not read; a Transform
  moves an instancer by its placement, not its points

## Alembic

Alembic files (`.abc`) open as USD layers, as `usdAbc` makes them in USD:
Load USD opens one, and a `.usda` can sublayer or reference one
(`@shot.abc@</char>`), with its time samples. `crates/xms-alembic` reads them
(the Ogawa kind every program has written since about 2013; not the older HDF5
kind); `xms-usd`'s `alembic` module turns what it reads into a layer, a format
registered with `openusd` (Imago's fork of it lets programs register formats).

- **Objects** are prims, their names made valid (`ns:pCube1` → `ns_pCube1`). A
  transform with one shape below it is one prim, the shape's, with the
  transform: Maya's `pCube1`/`pCubeShape1` is the Mesh `/pCube1`.
- **Transforms** are one `xformOp:transform`; one that does not inherit its
  parent's starts with `!resetXformStack!`.
- **Shapes:** PolyMesh and SubD are `Mesh` (left-handed, as Alembic winds
  polygons; a SubD's scheme, creases, corners and holes kept), curves
  `BasisCurves`, `NurbsCurves` or `HermiteCurves`, points `Points` (with ids
  and widths), cameras `Camera` (apertures from centimetres to millimetres),
  face sets `GeomSubset`s of the `materialBind` family.
- **Parameters:** `uv` is `primvars:st`, kept indexed; normals and widths are
  written with their indices applied; arbitrary parameters are `primvars:*`
  (a colour set is `color3f[]`, by its interpretation), user properties
  `userProperties:*`; visibility and bounds (`extent`) too.
- **Instances:** an instance's source is written once at the root, as an
  instanceable `over`, and referenced where it was and by every instance:
  drawn as USD instances.
- **Time:** times in seconds are time codes at the file's frames per second
  (`_ai_DCC_FPS`, else 24); the layer's start and end are the first and last
  sample of what changes. What does not change is a default value.
- **Not yet:** NURBS patches and lights (their objects are `Scope`s), and
  writing `.abc`. The whole file is read when the layer opens, so a long,
  heavy cache takes its memory at once.

## Example file

`examples/DeLorean.usdz`: 43 mesh prims, 461,595 triangles, 30 materials, 40
texture files. By VTX (<https://sketchfab.com/VTX_car>), CC BY-NC-SA 4.0. See
`examples/CREDITS.md`.
