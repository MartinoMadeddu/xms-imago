# USD

Load USD and Write USD are in the "File" sub-menu of the add-node menu. Pick, Prune, Set Attribute and Unpack are in "Primitives".

## Nodes

| Node | What it does |
|---|---|
| Load USD | Reads a `.usda`, `.usdc` or `.usdz` file, composed. Gives one primitive per mesh, curves or points prim. Its Mode: sublayered at the file's paths, or the file's default prim referenced or payloaded at a Primitive Location |
| Cube, Sphere, Grid | A shape, as one primitive at its Primitive Location |
| Merge | Any number of inputs as one: primitives side by side, or one mesh (its Mode) |
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
| Purpose | `render`, `proxy` and `guide`, inherited down the hierarchy. Which prim authors it is kept, so a purpose set nearer with Set Attribute wins over one a mesh only inherits |
| Other authored values | `kind` and `instanceable`, and every attribute a prim authors with one value (`doubleSided`, `subdivisionScheme`, a light's `inputs:intensity`, `userProperties:*`…), listed in the Primitive Inspector's Properties tab as coming from the file |
| Materials | Listed in the node's Parameters: colour, roughness, metallic, opacity, and the texture behind each input. UsdPreviewSurface, OpenPBR and Standard Surface. A material whose shaders are MaterialX or UsdPreviewSurface ones has **→ MaterialX**: a MaterialX operator with its network, to edit and bind in its place (see [MaterialX](materialx.md#from-a-usd-stage)) |
| Cameras | Primitives with no geometry: projection, focal length, aperture and offsets, clipping range, f-stop, focus distance, where they are. Moved by Transform, looked through in the Viewer |
| Lights | Primitives with no geometry: every UsdLux type (Dome, Rect, Sphere, Disk, Cylinder, Distant), with its values (intensity, exposure, colour, temperature, size, texture, shaping), where it is and which way it faces. Moved by Transform, listed in the Primitive Inspector (see [Lights](#lights)) |
| Skeletons | Listed by name |
| Time samples | Read at the stage's start time |

A file is read once and kept until it changes on disk. A `.usdz` is unpacked beside itself so its textures are files the Viewer can read.

## Big stages: boxes first

A stage is read on a thread of its own, in two passes over one open stage, so the program stays in use while it loads:

1. **Boxes.** The hierarchy, transforms, visibility, purpose and instancing, and each mesh, curves or points as the box of its authored `extent` (worked out from its points when it has none, so every prim with geometry has a box). The program waits for this pass, as it did for a whole load: there is nothing to show or work on yet. When it is done the Viewer and the Scene Explorer show the stage and the program is in use again; Transform, Prune and Set Attribute already work on it.
2. **The whole stage.** Every mesh array, primvar, material, binding and authored value, the geometry made on every core. When it is done it replaces the boxes, and everything downstream is cooked again.

Until the second pass is done, the Load USD node's parameters say *Shown as boxes; reading the geometry…*, an opened prim shows its box, the Primitive Inspector lists the boxes, and Write USD waits (it would otherwise write boxes in place of geometry). On Moana the boxes take a fraction of the time the whole island takes; `XMS_TIMINGS=1` prints both.

## Sublayer, Reference, Payload

Load USD's **Mode** says how the file comes into the scene, as in USD:

| Mode | In Imago | Written by Write USD |
|---|---|---|
| Sublayer | The whole stage, at the file's own paths | The file in the layer's `subLayers` |
| Reference | One prim of the file, with everything below it, at the **Primitive Location** (`/root` by default): `/Asset/geo/body` is `/root/geo/body`. The prim is the file's `defaultPrim`, or the one named in **Prim** (Pick lists the root prims). A file naming no default prim gives its first root prim, and the panel says so | `def "root" (prepend references = @file@</Asset>)`, with the edits under it as `over`s |
| Payload | As Reference: Imago always loads it | `prepend payload = @file@</Asset>`: other programs load it on demand |

- The prims above the Primitive Location are Xforms, made by Imago (`/shot/set` puts `shot` above). What the file has outside the referenced prim is left out, as USD leaves it out.
- Bindings and point-instancer prototypes inside the referenced prim follow it to its new paths. A binding to a material outside it keeps its path.
- A file's axes and unit are not converted by a reference in USD. Imago shows every stage Y up in metres, so where the file's differ from the layer's, Write USD puts the transform from one to the other on the referenced prim, on top of the prim's own: opened, the layer gives what Imago shows.
- The Scene Explorer lists the stage at its new paths; Set Attribute, Material Binding and every Prims Location tab name it by them.

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

What is set on a group is inherited by the primitives below it, as USD does: `visibility` invisible and `active` off reach everything below; `purpose` comes from the nearest prim that has one, the primitive itself first, whether the file authors it there or Set Attribute set it there (on the same prim, Set Attribute's wins); `kind`, `doubleSided` and the other Properties are not inherited; an Object value reaches each primitive below that has no attribute of that name, the nearest group winning. The Primitive Inspector's Object tab lists inherited values with the primitive's own; its Properties tab lists a prim's properties and says, for each, whether it comes from the file, from Imago, from which Set Attribute operator, or from which group above it. ICE reads only a primitive's own attributes. Write USD writes each value once, on the group.

Values per point, per face or per corner are not set here: changing them rewrites the primitive's arrays, and that is ICE's.

## Shapes are primitives

Everything in the scene is a primitive, so Cube, Sphere and Grid each make one, at their **Primitive Location**: `/cube`, `/sphere` or `/grid`, or any path typed there, such as `/root/asset/cube1`. The parents on the path are Xforms, as in Solaris. What is typed is made a valid USD path: empty segments are dropped, and characters a USD name cannot hold become `_` ("Used as …" under the field says so).

Merge joins its inputs, in order, by its **Mode**. It takes any number: wiring its last input adds another, so one is always free (Inputs, in its Parameters, sets how many). One Merge does what a chain of them did.

| Mode | What it does |
|---|---|
| Auto | Goes by the first input: primitives are merged as primitives, a plain mesh or a clip is flattened |
| Primitives | The primitives of every input side by side, each keeping its path. A plain mesh among them becomes a primitive at `/mesh` |
| Flatten | Everything as one plain mesh, as before primitives were merged, and as Unpack does |

On a path two inputs have, the later input is the stronger, as an `over` on the same path in USD: its primitive takes the first one's place. What it does not say is read from the first one: a shape put on the path of a prim loaded from a file keeps that prim's material and stage, and is written as a change to it.

The pattern is a name pattern as described in [Interface](interface.md#name-patterns): comma-separated regular expressions matched against the full prim path, with a "Pick" list of the paths coming in.

![One wheel picked out of a 461,595 triangle model and moved](xms_usd_pick.png)

## Instancing

Native instances and point instancers keep their instancing. The geometry of a prototype is held once and each copy is a placement: a native instance gives one primitive per mesh, under the instance's own path; a point instancer gives one primitive per prototype mesh, holding all of its points.

- Transform moves the placements, not the points, so moving a forest is moving matrices.
- A node that changes geometry (Edit Poly, the UV nodes) on one instance makes that instance a mesh of its own. The other copies stay shared.
- The Viewer draws a mesh with many copies in one draw call (GPU instancing), in its material's colour. A mesh with 64 copies or fewer is drawn copy by copy with its full material and textures.

A point instancer is also a primitive of its own, at its path: a points primitive with one point per instance and the instancer's arrays as Point attributes (`points` from `positions`, `orient`, `scale`, `protoIndex`, `id`), in its own space. It is not drawn (its prototypes are, at its points) and not merged into a mesh. Selecting it in the Scene Explorer lists its instances in the Primitive Inspector's Point tab, one row per instance, and nothing else. A group above it, or the whole scene, lists it the same way: its prototypes' geometry is listed only when a prototype (a row below the instancer) is selected; its Properties tab gives the number of instances and its prototypes.

Its points are where the copies are: whatever changes them (an ICE graph moving `P`, a Transform on the instancer alone, through its Prims Location tab or a Pick) moves the copies, which are made again from the points after every operator, only when the instancer changed. Write USD writes what changed as the instancer's own arrays (`positions`, `orientations`, `scales`, `protoIndices`, `ids`), and an instancer moved as a whole gets its own transform; its prototypes' copies are not edits of their own.

A prototype is listed where the stage's hierarchy has it, under the instance it is seen through when the instancer is inside a native instance (the corals of Moana's island), so the Scene Explorer lists each prim once. Should a drawn prim still not be where the hierarchy lists it, the node's "Not read in full" says how many and gives the first.

**Nested point instancers**, a point instancer inside a prototype of another (grass tufts scattered by a field instancer, each tuft its own instancer of blades), are drawn through the outer one, as USD composes them, down to four deep. The inner instancer is a points primitive of its own, placed once per point of the outer instancer using its prototype; its prototypes' meshes are drawn at each of its points in every one of those copies. Changing the outer instancer's points moves the inner instancer's copies and, a level down, what they draw. Write USD lists edits of an inner instancer as not written, as for any prototype.

The Primitive Inspector lists instanced geometry once, in its own space, not once per copy: a point instancer's prototypes, and the meshes of native instances. Their Properties tab gives the number of copies. What it lists grows with the geometry there is, not with what is drawn, so a stage of millions of instanced points opens in it at once.

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
- PNG, JPEG, TIFF, TGA, HDR and OpenEXR are read. A float image (EXR, HDR) holds linear light and is shown as such.
- **UDIM tiles:** a texture path with `<UDIM>` (or `<udim>`) in place of the tile number (`body.<UDIM>.exr`) reads every tile there is, from 1001 to 1999, as Mari, Substance and USD name them. The tiles are laid out as they lie in UV space and each part of the mesh shows the tile its UVs fall in; a tile not there is grey. The tiles share 4096 pixels across the widest way (1,024 each for up to four, 409 for ten). It works for a USD stage's textures, the Material operator's and a MaterialX graph's alike. In the file browser, with **Collapse sequences** and **UDIM** on, the tiles are one entry, `body.<UDIM>.exr`, which picking gives as it is.
- Images larger than 2048 pixels on a side are reduced to that. Each gets its chain of smaller copies, so distant surfaces do not shimmer. Textures repeat outside the unit square.
- A material with opacity below one is blended. A material whose opacity comes from its colour texture is cut out by that texture's alpha.
- "Textures" in the Viewer menu turns all of this off and shows plain grey.

## Making materials

The **Material** operator (Add menu, Materials) makes a UsdPreviewSurface material. Its parameters are the surface's inputs: diffuse and emissive colour, metallic, roughness, opacity, opacity threshold, IOR, clearcoat and its roughness, with UsdPreviewSurface's defaults. A Textures group takes an image file for diffuse, emissive, metallic, roughness, opacity, normal and occlusion; an input fed by a texture is greyed out, its value coming from the texture. An opacity file that is the diffuse file reads that texture's alpha.

The material is named by its operator: rename the node to rename it (spaces and other characters USD does not take become `_`). Viewed on its own, it is shown on a sphere, with its colour, opacity and textures in Textured mode, and the Scene Explorer lists it as one row.

It puts out a material, not a scene: Material Binding puts it in the scene and binds it. In USD it is a `Material` holding a `UsdPreviewSurface` shader, a `UsdUVTexture` per file (colours read as sRGB, the rest raw, normal maps scaled to -1..1) and a `UsdPrimvarReader_float2` reading `st`.

## Binding materials

**Material Binding** (Add menu, Materials) takes the scene on its top input and materials on its left side, one per input, numbered. A Material's output is on its right side; sockets carrying a material are purple. Wiring the last material input adds another, so one is always free; Inputs, in its Parameters, sets how many.

- **Material Library Location** (`/materials` by default): every material wired in goes there, by its operator's name (`/materials/CarPaint`), as a `Material` prim with no geometry. Its parents are Scopes. A material already on that path is replaced. Two inputs whose materials have one name: the first is used, and the Parameters say so.
- **Rows** are on its **Bindings** tab (the Prims Location tab of the other operators): each row takes the primitive at its path and every one below it, as on Transform's tab, and binds its material to them. A row's Material column says which: the input of the row's number unless another input with a material wired in is chosen, or **Location**: a material already in the scene (read from a file, or put there by an earlier Material Binding), by its path. Location opens a line under the row for that path: type it, pick it from the materials of the scene coming in, or drop the material from the Scene Explorer. On a primitive two rows take, the later row wins. No rows: nothing is bound.
- A row on a group binds each primitive below it, written on each one: what the file bound below it is replaced. A point instancer is not bound itself; its prototypes, below it, are.
- In the Viewer, Textured mode shows each primitive in its new material. A material of the file that no primitive wore before is shown grey for now.
- The Scene Explorer lists the materials under the library, of type Material. Selected, the Primitive Inspector's Properties tab lists the surface's inputs, from the Material operator that made it; a bound primitive's `material:binding` row comes from the Material Binding that bound it, and Go to Operator selects either.
- Write USD writes the materials as `def Material`s with their shaders, under `def Scope` parents, and each binding as `rel material:binding` (with `MaterialBindingAPI`) on the primitive.

## Cameras

A camera is a primitive with no geometry: its lens and gate as USD has them, and its placement, looking down its -Z axis with +Y up. Load USD gives a file's cameras; the **Camera** operator (Add menu, Create) makes one at its Primitive Location (`/camera` by default), with Translate and Rotate, Projection, Focal length, Filmback (the gate; Filmback Presets: USD default, full frame, Super 35, ALEXA Mini 16:9, ALEXA Mini Open Gate, ALEXA Mini LF Open Gate), Filmback offset, Clipping, f-stop and Focus distance. Focal length and gate are in tenths of a scene unit, as in USD: millimetres in a centimetre scene. The field of view comes from their ratio and is shown under them.

- Transform moves a camera, as any primitive; Merge joins it to a scene; Set Attribute sets its attributes.
- The Viewer draws each camera as a pyramid from its eye to its gate, as long as its lens.
- **Looking through:** the Viewer's camera menu (🎥 Perspective, next to the display mode) lists the cameras of the scene; choose one to look through it, Perspective to go back to the Viewer's own camera as it was. A Camera operator's **Look through** does the same. The whole gate is shown, what lies outside it darkened, the camera's name at the top, in the middle. Orbiting, panning, zooming or framing leaves the camera: the Viewer's own takes its place, where it was, with its field of view, turning about its focus distance (or 5 m).
- The Primitive Inspector's Properties tab lists a camera's values; Go to Operator selects the Camera operator that made it.
- Write USD writes a camera made in Imago as `def Camera` with its transform and lens (`projection`, `focalLength`, `horizontalAperture`, `verticalAperture` and `clippingRange` always, the rest when set), and a file's camera that moved as its new transform. A pruned camera is deactivated.

## Lights

A light is a primitive with no geometry, as a camera is: its UsdLux type, its values, and its placement, Rect, Disk and Distant lights emitting along their -Z. Load USD gives a file's lights; the **Light** operator makes one.

**The Light operator** is one operator with a **Type**: Dome, Rect, Point, Sphere, Disk, Cylinder, Distant (a point light is a sphere light with **Treat as Point** on, as USD has it: UsdLux has no point light of its own). The add menu's **Lights** category has an entry for each, the same operator set to that type: Dome Light, Rect Light, Point Light… Changing its Type keeps the values both types have, and its Primitive Location when it was the type's own (`/lights/dome` becomes `/lights/rect`).

- **Primitive Location**: `/lights/dome`, `/lights/rect`, `/lights/point`… by type. **Translate** and **Rotate**, as the Camera's, with the Viewer's handles. A new light is 2 m up and 4 m back, turned 30° down; a dome at the origin (where a dome is does not matter, its rotation turns the environment).
- **Light**: Intensity, Exposure (stops), Colour, Use Temperature and Temperature (kelvin), Normalize. **Contribution**: Diffuse and Specular multipliers. **Shape**: the type's own: a dome's Texture, Format (automatic, latlong, mirroredBall, angular, cubeMapVerticalCross), Pole Axis and Guide Radius (how big it is drawn: 1 on a new Light operator, UsdLux's fallback being 100000); a rect's Width, Height and Texture; a sphere's Radius and Treat as Point; a disk's Radius; a cylinder's Length, Radius and Treat as Line; a distant light's Angle.
- Each value starts at UsdLux's fallback (1 for intensity, 50000 for a distant light's, 6500 K, 0.53° for the sun's angle); one changed has a dot after its label and its fallback in its tooltip. **Reset to the schema's** puts them all back.

The parameters are laid out from `assets/usdlux.json`, written from OpenUSD's schemas: LightAPI's attributes, then each type's, with their USD names, types, fallbacks, groups and help. Write USD writes a light made in Imago as its type (`def DomeLight "dome"`) with its transform and every value (`float inputs:intensity = 1`, `uniform token poleAxis = "scene"`…), an empty texture left out; a file's light that moved, as its new transform. The Primitive Inspector's Properties tab lists a light's values; Go to Operator selects the Light operator that made it.

**In the Viewer**, each light is drawn as its shape, in its colour (times its temperature's when that is on, brought to full brightness so a dim light still shows):

| Type | Drawn as |
|---|---|
| Dome | Half a sphere, as its name says: two arches and a ring at 45 degrees over a ground circle, with its pole, and the compass on the ground (N along -Z, with the longer tick, E along +X, S, W). At its guide radius, but at most 12 m (a file's dome at UsdLux's 1 km would be past the far plane); with Pole Axis Z, its top is along +Z. Its placement's scale is not a size |
| Rect | Its rectangle, width by height, with its diagonals and an arrow along -Z, the way it emits |
| Point | A small star |
| Sphere | Three circles at its radius |
| Disk | Its circle, and an arrow along -Z |
| Cylinder | A circle at each end, along X, and four lines between |
| Distant | A small circle and five parallel arrows along -Z |

Click a light's lines to select it, as a mesh is selected; selected, its lines turn the selection's colour, and a Light operator selected has the Move and Rotate handles. Cameras are picked by their pyramid the same way. A file field (a dome's Texture, a rect's) has 📂 to browse for the image (EXR, HDR, PNG, JPEG, TIFF, TGA) and ✕ to clear it.

Not yet: lighting the Viewer with them, a dome's image as its environment, a file's light's values edited, ShapingAPI, ShadowAPI and portals in the operator.

## Write USD

Write USD writes what the scene graph changed as a `.usda` override layer. The layer sublayers the file each sublayered stage was read from, references or payloads each stage loaded that way on its prim, and holds only opinions over them, so opened on its own (in usdview, rray, or Imago) it gives what Imago shows. It repeats the first sublayered file's `upAxis` and `metersPerUnit`, which USD reads from the layer that is opened; with none, it is Y up in metres.

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
| A stage loaded as Reference or Payload | `def "root"` (its Primitive Location) with `prepend references` or `prepend payload` to the file's prim, and the axis and unit transform where needed; its parents `def Xform` |
| Material binding or purpose changed | `material:binding`, `purpose` |
| A material Material Binding put in the scene | `def Material` with its `UsdPreviewSurface`, `UsdUVTexture` and `UsdPrimvarReader_float2` shaders, under `def Scope` parents |
| A point instancer's points or their attributes | `positions`, `orientations`, `scales`, `protoIndices`, `ids` |
| Set Attribute values | Object: `primvars:name` with `interpolation = "constant"`. Property: an attribute (`token visibility`, `uniform bool doubleSided`, `uniform token subdivisionScheme`, any other name with its type) or metadata (`active`, `kind`, `instanceable`). On any prim of the stage: a group, a camera, a light |
| A light made by the Light operator | `def DomeLight`, `def RectLight`… with `xformOp:transform` and every UsdLux value |
| A camera made by the Camera operator | `def Camera` with `xformOp:transform`, `projection`, `focalLength`, the apertures, `clippingRange`, and `fStop`, `focusDistance` and the offsets when set |
| Render prims the render operators made | `def RenderSettings`, `def RenderProduct`, `def RenderVar` under `def Scope` parents, with their attributes and relationships; the layer names the RenderSettings prim (`renderSettingsPrimPath`). One of a loaded file: an `over`, its relationships appended. See [Rendering](render.md) |
| A prim made in Imago (a shape, say) | A new prim (`def Mesh`, `def BasisCurves` or `def Points`), its parents `def Xform` where no stage has them |

The layer's `defaultPrim` is the first root prim of the first stage, or, with no stage, the first prim made in Imago (`cube` for a lone Cube). A layer with no stage is Y up in metres, as Imago is; next to a stage, prims made in Imago are written in the stage's axes and unit.

Only what changed is written: a stage nothing touched gives a layer with its header alone. What changed is found by comparison with the stage as it was loaded (see [Geometry](geometry.md)).

Not written yet, and listed by the panel:

- Changes to prims inside a native instance. USD keeps instance proxies read-only: the prototype has to be edited, or the instance made a prim of its own.
- Changes to a point instancer's prototypes, which go through the instancer's positions, orientations and scales.

Values are written at the current frame, as single values.

## Not done yet

- Load USD: a layer offset on a reference, several references on one prim, population masks, payloads left unloaded; moving the referenced prim itself when it is a mesh is written without the axis and unit transform

- Write USD: time samples over a frame range, editing inside instances, a flattened single file; the stage's own `defaultPrim` (the first root prim is used)
- A Primitive Location for the operators that make a plain mesh (Unpack, Merge set to Flatten); until then it has no path, and Write USD says so. ICE keeps the primitives it is given, with their paths; a plain mesh going into it stays plain
- Normal, roughness, metallic and occlusion maps in the Viewer
- Cameras: gate offsets and depth of field in the Viewer, cameras inside native instances, animated cameras, editing a file's camera's lens
- Animation: time samples beyond the start time, skeletal animation, skinning, blend shapes
- Per-face material subsets, creases and holes (kept in the file, not shown), NURBS, volumes, implicit shapes (sphere, cube, cylinder…)
- Point instancers nested more than four deep (listed in the node's "Not read in full")
- Materials with a graph inside (compounds); nodes to edit materials read from a file, cameras or the hierarchy
- Material Binding: binding a group as a group (one `material:binding` on it, inherited below, with `bindMaterialAs` strength), binding purposes (`preview`, `full`), collections; the colour of a file's material no primitive wore before
- Set Attribute: time samples (values are default values), arrays, inherited Object values read by ICE
- Arrays, dictionaries (`customData`) and relationships other than the material binding are not listed in the Properties tab
- Write USD writes nothing for a stage none of whose primitives is left (all pruned, or all below a deactivated group)
- A point instancer's `invisibleIds` are kept (those instances are not drawn) but not listed; `velocities` and `accelerations` are not read; a Transform moves an instancer by its placement, not its points

## Example file

`examples/DeLorean.usdz`: 43 mesh prims, 461,595 triangles, 30 materials, 40 texture files. By VTX (<https://sketchfab.com/VTX_car>), CC BY-NC-SA 4.0. See `examples/CREDITS.md`.
