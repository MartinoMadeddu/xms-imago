# USD

Load USD and Write USD are in the "File" sub-menu of the add-node menu. Pick, Prune, Set Attribute and Unpack are in "Primitives".

## Nodes

| Node | What it does |
|---|---|
| Load USD | Reads a `.usda`, `.usdc` or `.usdz` file, composed. Gives one primitive per mesh, curves or points prim |
| Cube, Sphere, Grid | A shape, as one primitive at its Primitive Location |
| Merge | Two inputs as one: primitives side by side, or one mesh (its Mode) |
| Pick Primitives | Marks the primitives whose path matches a pattern as picked |
| Prune Primitives | Removes the primitives its Prims Location tab lists, or keeps only those |
| Set Attribute | Sets one value per prim: a constant primvar (Object) or the prim's own attribute or metadata (Property): `visibility`, `purpose`, `active`, `kind`, `instanceable`, `subdivisionScheme`, `doubleSided`, or any name |
| Unpack | Merges every primitive into one mesh |
| Write USD | Writes what the scene graph changed in its stages as a USD override layer |

## Composed stages

A file is read as a stage, with its composition resolved: sublayers, references, payloads, inherits, specializes and variants, in text and binary layers alike. What you see is what usdview would show.

Every prim keeps what the file says about it. Geometry stays in its own space, as authored, and each primitive is placed by its world transform. Nothing is baked into the points, so what is written back matches the file.

| | |
|---|---|
| Formats | Text (`.usda`), binary (`.usdc`) and `.usdz` |
| Composition | Sublayers, references, payloads, inherits, specializes, variants |
| Transforms | The whole `xformOpOrder`, `!resetXformStack!` included |
| Units and axes | Z-up and `metersPerUnit` become a correction on the placement: the points stay as authored |
| Meshes | Points, polygons of any size, left-handed orientation, subdivision scheme |
| Curves | `BasisCurves`: control points, counts, basis (linear, Bézier, B-spline, Catmull-Rom) and wrap, kept as authored |
| Points | `Points`, with their widths |
| Primvars | Every numeric primvar, normals and widths included, with its interpolation and indices |
| Instancing | Native instances and point instancers: each prototype's geometry is held once and shared by every copy |
| Purpose | `render`, `proxy` and `guide`, inherited down the hierarchy |
| Materials | Listed in the node's Parameters: colour, roughness, metallic, opacity, and the texture behind each input. UsdPreviewSurface, OpenPBR and Standard Surface |
| Cameras | Listed: projection, focal length, aperture, clipping range, position |
| Skeletons, lights | Listed by name |
| Time samples | Read at the stage's start time |

A file is read once and kept until it changes on disk. A `.usdz` is unpacked beside itself so its textures are files the Viewer can read.

The Parameters of a Load USD node show the stage: primitive and triangle counts (every instanced copy counted), up axis and unit, a count of prims by type, the materials with their textures, the cameras, skeletons and lights, and a "Not read in full" list for whatever the file holds that is not handled yet.

## Primitives

A stage comes in as primitives: each mesh, curves or points prim stays a separate piece, with its path, material binding and purpose, and is passed from node to node without being copied. Nothing is merged until a node needs one mesh.

Transform, Edit Poly and Prune Primitives say which pieces they work on in their **Prims Location** tab: a list of paths, a path taking its prim and everything below it (see [Interface](interface.md#parameters)). An empty list means every piece. Pick Primitives still chooses pieces for the operators after it with an empty list, as before. Either way, on the chosen pieces:

- Transform moves each picked primitive on its own, by its placement. The geometry is not copied.
- Edit Poly, UV Unwrap, UV Transform and UV Edit on one picked primitive edit it where it is drawn, and it stays itself: same path, same material, same placement. Only what the node changed is replaced: a UV node leaves the points shared with the file.
- Several picked primitives go through Edit Poly and the UV nodes merged, as one primitive in the place of the first.
- Every other primitive passes through untouched.

Without a pick, Transform moves each primitive, and Edit Poly and the UV nodes edit a lone primitive where it is drawn, keeping it itself. Several primitives with none picked go through those nodes merged into one mesh.

An ICE node runs its graph on each primitive (or each picked one) in the primitive's own space, as an ICE tree runs on an object in Softimage.

## Set Attribute

Set Attribute is the scene graph's one operator for setting values on prims. It holds rows; each row sets one value:

| Context | Written as | For |
|---|---|---|
| **Object** | `primvars:name`, `interpolation = "constant"` | Data for rendering: shaders read it by name, and the prims below a group inherit it. `displayColor`, `displayOpacity`, a shader variation |
| **Property** | The prim's own attribute or metadata, by name | What the prim is and how it is used: `visibility`, `purpose`, `active`, `kind`, `instanceable`, `subdivisionScheme`, `doubleSided`, a camera's `focalLength`, `userProperties:*` |

Each row has a context, a name, a type and a value. **Pick** next to the name offers the common names, each with its type and, where USD fixes them, its values (`visibility` is `inherited` or `invisible`). Any other name can be typed, with its type chosen: `bool`, `int`, `float`, `double`, `string`, `token`, `float2`, `float3`, `float4`, `color3f`, `asset`. An Object value takes the types a primvar column holds (no `double`, `string` or `asset`). Where two rows set the same name, the later one wins.

The Prims Location tab says where. Here a path **names** that prim alone, of any type: a group, a camera or a light as well as a mesh, so `kind` on `/DeLorean` is one opinion on `/DeLorean`, not one on each of its meshes. `/set/*` names the children of `/set`, `/set/**` `/set` and everything below it. With no location, the rows are set on every primitive coming in (or the picked ones).

Any prim can be named: a mesh, a group or camera of a loaded stage, or one of the Xforms Imago makes above a shape's Primitive Location (`/root/asset/geo` above `/root/asset/geo/cube`).

In Imago:

- An Object value on a primitive is an Object attribute of it: the Primitive Inspector's Object tab shows it, ICE reads it.
- `purpose` is the primitive's purpose: the Scene Explorer's purpose toggles show or hide it.
- `active` off takes the prim and everything below it out of the scene.
- `visibility` invisible hides the prim and everything below it in the Viewer.
- The rest is kept on the prim and written by Write USD.

What is set on a group is inherited by the primitives below it, as USD does: `visibility` invisible and `active` off reach everything below; `purpose` is theirs where they have none of their own; an Object value reaches each primitive below that has no attribute of that name, the nearest group winning. The Primitive Inspector's Object tab lists inherited values with the primitive's own. ICE reads only a primitive's own attributes. Write USD writes each value once, on the group.

Values per point, per face or per corner are not set here: changing them rewrites the primitive's arrays, and that is ICE's.

## Shapes are primitives

Everything in the scene is a primitive, so Cube, Sphere and Grid each make one, at their **Primitive Location**: `/cube`, `/sphere` or `/grid`, or any path typed there, such as `/root/asset/cube1`. The parents on the path are Xforms, as in Solaris. What is typed is made a valid USD path: empty segments are dropped, and characters a USD name cannot hold become `_` ("Used as …" under the field says so).

Merge joins two inputs by its **Mode**:

| Mode | What it does |
|---|---|
| Auto | Goes by the first input: primitives are merged as primitives, a plain mesh or a clip is flattened |
| Primitives | The primitives of both inputs side by side, each keeping its path. A plain mesh among them becomes a primitive at `/mesh` |
| Flatten | Everything as one plain mesh, as before primitives were merged, and as Unpack does |

On a path both inputs have, the second input is the stronger, as an `over` on the same path in USD: its primitive takes the first one's place. What it does not say is read from the first one: a shape put on the path of a prim loaded from a file keeps that prim's material and stage, and is written as a change to it.

The pattern is a name pattern as described in [Interface](interface.md#name-patterns): comma-separated regular expressions matched against the full prim path, with a "Pick" list of the paths coming in.

![One wheel picked out of a 461,595 triangle model and moved](xms_usd_pick.png)

## Instancing

Native instances and point instancers keep their instancing. The geometry of a prototype is held once and each copy is a placement: a native instance gives one primitive per mesh, under the instance's own path; a point instancer gives one primitive per prototype mesh, holding all of its points.

- Transform moves the placements, not the points, so moving a forest is moving matrices.
- A node that changes geometry (Edit Poly, the UV nodes) on one instance makes that instance a mesh of its own. The other copies stay shared.
- The Viewer draws a mesh with many copies in one draw call (GPU instancing), in its material's colour. A mesh with 64 copies or fewer is drawn copy by copy with its full material and textures.

## The Scene Explorer and the Viewer

The Scene Explorer lists a stage as its hierarchy: groups, meshes, curves, points, materials, cameras, lights and skeletons, each with its icon. Pruned primitives leave the list, and so do groups left empty.

What is drawn follows the explorer:

- A closed prim is drawn as the box around everything below it. The island first shows as one box per root prim.
- An open prim hands the decision to its children.
- A mesh, curves or points prim whose parent is open is drawn as geometry.
- The square at the right of a row draws that prim and everything below it as geometry, however far it is opened.
- "Show all geometry" in the explorer's header draws everything.

Opening and closing does not cook the graph again. The boxes come from the primitives as they reach the Viewer, so they follow a Transform or a Prune upstream. A point instancer's box covers every copy, and an open instancer shows one box per prototype.

The **guide**, **proxy** and **render** toggles in the explorer's header choose which purposes are drawn. By default guides are hidden, proxies are shown, and render geometry is shown where its asset has no proxy (where it has one, the proxy stands in for it). A prim with no purpose is always drawn.

Curves are drawn as lines through their basis, points as small crosses sized by their width.

## Materials in the Viewer

Primitives are drawn with their materials: colour, roughness, metallic, opacity, the colour texture and the emissive texture. Textures packed in a `.usdz` are used from where it was unpacked.

![The example model with its textures](xms_usd_textured.png)

- Texture files are decoded in the background. A surface shows its plain colour until its texture is ready, and "loading N" shows next to the Viewer menu meanwhile.
- Images larger than 2048 pixels on a side are reduced to that. Each gets its chain of smaller copies, so distant surfaces do not shimmer. Textures repeat outside the unit square.
- A material with opacity below one is blended. A material whose opacity comes from its colour texture is cut out by that texture's alpha.
- "Textures" in the Viewer menu turns all of this off and shows plain grey.

## Write USD

Write USD writes what the scene graph changed as a `.usda` override layer. The layer sublayers the file each stage was read from and holds only opinions over it, so opened on its own (in usdview, rray, or Imago) it gives what Imago shows. It repeats the file's `upAxis` and `metersPerUnit`, which USD reads from the layer that is opened.

Set a path and press **Write**. The panel then says what was written, and lists what could not be, with the reason.

| Changed in Imago | Written |
|---|---|
| A prim moved | Its own transform (`xformOp:transform`), so it still follows its parents |
| Points moved | `points`, and `extent` to match |
| Polygons or curves changed | `faceVertexCounts` and `faceVertexIndices`, `orientation`; for curves their counts, type, basis and wrap |
| Primvars written or added | `primvars:` with their USD type (`texCoord2f`, `color3f`, `normal3f`…), interpolation and indices |
| Attributes lost | Blocked (`= None`), so the file's value is not seen through |
| Several prims merged into one | The whole geometry, in the prim's own space |
| A prim pruned | `active = false` |
| Material binding or purpose changed | `material:binding`, `purpose` |
| Set Attribute values | Object: `primvars:name` with `interpolation = "constant"`. Property: an attribute (`token visibility`, `uniform bool doubleSided`, `uniform token subdivisionScheme`, any other name with its type) or metadata (`active`, `kind`, `instanceable`). On any prim of the stage: a group, a camera, a light |
| A prim made in Imago (a shape, say) | A new prim (`def Mesh`, `def BasisCurves` or `def Points`), its parents `def Xform` where no stage has them |

The layer's `defaultPrim` is the first root prim of the first stage, or, with no stage, the first prim made in Imago (`cube` for a lone Cube). A layer with no stage is Y up in metres, as Imago is; next to a stage, prims made in Imago are written in the stage's axes and unit.

Only what changed is written: a stage nothing touched gives a layer with its header alone. What changed is found by comparison with the stage as it was loaded (see [Geometry](geometry.md)).

Not written yet, and listed by the panel:

- Changes to prims inside a native instance. USD keeps instance proxies read-only: the prototype has to be edited, or the instance made a prim of its own.
- Changes to a point instancer's prototypes, which go through the instancer's positions, orientations and scales.

Values are written at the current frame, as single values.

## Not done yet

- Write USD: time samples over a frame range, editing inside instances, a flattened single file; the stage's own `defaultPrim` (the first root prim is used)
- A Primitive Location for the other operators that make geometry (ICE, Unpack, Merge set to Flatten); until then their plain mesh has no path, and Write USD says so
- Normal, roughness, metallic and occlusion maps in the Viewer
- Looking through a USD camera
- Animation: time samples beyond the start time, skeletal animation, skinning, blend shapes
- Per-face material subsets, creases and holes (kept in the file, not shown), NURBS, volumes, implicit shapes (sphere, cube, cylinder…)
- Nested point instancers
- Nodes to edit materials, cameras or the hierarchy
- Set Attribute: time samples (values are default values), arrays, Property values in the Primitive Inspector, inherited Object values read by ICE
- Write USD writes nothing for a stage none of whose primitives is left (all pruned, or all below a deactivated group)

## Example file

`examples/DeLorean.usdz`: 43 mesh prims, 461,595 triangles, 30 materials, 40 texture files. By VTX (<https://sketchfab.com/VTX_car>), CC BY-NC-SA 4.0. See `examples/CREDITS.md`.
