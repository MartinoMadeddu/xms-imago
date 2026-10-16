# XMS Core Operator Specification

**Status:** Draft v0.3 (15/10/2026), checked against `main` at 30a928e
**Scope:** The minimum operator set needed to load, modify, author and render OpenUSD scenes.
**Principle:** One operator per USD concept (prims, properties, composition arcs, collections, render prims). Operators edit a USD scene; they do not translate it into a separate scene format.

Words follow `VOCABULARY.md`: a node of the scene graph is an **operator**, its settings are **parameters** (in the Parameters pane), an object in the stage is a **primitive** (prim for short). Each operator below has a **Status** in Imago today:

| Status | Meaning |
|---|---|
| **Done** | The operator exists and works as described |
| **Partial** | It exists, with less than described (what is missing is listed) |
| **Read only** | Imago reads and keeps this from USD files, but no operator authors it yet |
| **Not yet** | Neither |

---

## 1. Conventions

### 1.1 What flows between operators

Every operator input and output carries one of three things (`EvalResult`):

| Kind | Meaning | Made by |
|---|---|---|
| **Primitives** | USD primitives, each with its path, geometry (`Geo`), placement, material binding and purpose. Prims read from a file keep their stage and what they were loaded as, so changes are written back as opinions over the file | Load USD, Cube, Sphere, Grid, Load FBX (Geometry output), Merge, Transform, Edit Poly, Prune |
| **Plain mesh** | One mesh with no path: not yet a primitive | Unpack, Merge set to Flatten, ICE on a plain mesh |
| **Clip** | A skeleton and its animation (FBX takes, mocap) | Load FBX (Clip, Rest), Test Clip, the animation and mocap operators |

There is no separate "packed" state: "packed" was dropped from the vocabulary, and everything that has a path is a primitive. FBX meshes come in as primitives too; only clips stay outside USD for now.

The scene is what the operator with the **view flag** puts out (the viewed output, for operators with several). There is no Output operator: one operator always has the view flag, as Houdini's display flag. The Viewer draws that scene, the Scene Explorer lists it as its tree and the Primitive Inspector shows it.

### 1.2 Merge rule

Status: **Done** (any number of inputs, one kept free).

The **first input drives** the result. Merge has a **Mode**: Auto (by the first input), Primitives, Flatten.

- First input **primitives** (Auto): the inputs are merged as primitives, each keeping its path. A plain mesh among them becomes a primitive at `/mesh`.
- First input a **plain mesh or a clip** (Auto): everything is flattened into one plain mesh, as Unpack does.
- On a path two inputs have, the **later input is stronger**, as an `over` on the same path: its prim takes the first one's place, and what it leaves unset (material, stage, purpose) is read from the first. A shape put on the path of a file's prim is written as a change to that prim.

### 1.3 Targeting model

All targeting reduces to two parameter types:

| Type | Used by | Status | Description |
|---|---|---|---|
| **Primitive Location** | Create operators (Cube, Sphere, Grid) | **Done** | One path that sets where the new prim is, e.g. `/root/char/geo`. Typed text is made a valid USD path. Parents on the path are Xforms |
| **Primitive Locations** | Operators that work on a subset of the scene | **Done** on Transform, Edit Poly, Prune | An ordered list, one path per row, in the operator's **Prims Location** tab (second tab of the Parameters pane). Each row shows how many incoming prims it matches. Filled by typing, by **+ Add Location** (the rows selected in the Scene Explorer), by Pick (the paths coming in), or by dragging prims from the Scene Explorer onto the operator, its tab or + Add Location. Several rows can be selected and dragged at once (Ctrl/Cmd, Shift) |

List entries accept:

| Entry | Status |
|---|---|
| Absolute paths: `/root/char/geo/head`. A path takes that prim **and everything below it** | **Done** |
| Wildcards: `*` any run of characters within a name, `?` one character, `**` any number of levels: `/root/*/*/glass`, `/set/**/lamp*`. `/` takes everything | **Done** |
| Collection references (section 8) | Not yet |
| Exclusion entries (proposed: prefix `-`) | Not yet |

Rules:

- An **empty list means every primitive**: the operator works as it would with no list. Exception: Prune with an empty list prunes nothing.
- The list belongs to its operator. The operators after it see the primitives as they came in, not the list.
- Pick Primitives, the older way of choosing prims for the operators below it, stays for older graphs. An operator with an empty list still works on what an upstream Pick chose.
- The operators that have the Prims Location tab are listed by `core::locations::takes_locations`. Each subset-aware operator in this spec adds itself there.
- An operator that sets a value on a prim **names** prims instead (`core::locations::names_prims`, Set Attribute): a path is that prim alone, of any type (a group, a camera, a light as well as a mesh), so a value on a group is one opinion on the group. `/set/*` names the children of `/set`, `/set/**` `/set` and everything below it. **Done**

> One convention covers Prune, Material Assign, Set Attribute, Variant Select and every other subset-aware operator.

### 1.4 Compounds

Status: **Not yet** for the scene graph.

- Any operator network can be wrapped in a **Compound**.
- Compounds expose chosen parameters and can be dived into.
- **Nested compounds are supported.**
- Materials are compounds (section 9).

Today the only graph inside an operator is the **ICE graph** of an ICE operator: it runs on each primitive (or the picked ones) in the primitive's own space and writes attributes. Scene-graph compounds should be dived into the same way as ICE graphs: double-click, or ↓ and ↑.

### 1.5 Operator definition template

Each operator below is described with: **Purpose**, **Inputs**, **Parameters**, **Targeting** (Primitive Location / Primitive Locations / none), **USD mapping** and **Status**.

---

## 2. Stage I/O

### 2.1 Load USD
- **Purpose:** Open a layer from disk and output its stage as primitives.
- **Inputs:** none
- **Parameters:** file path; load mode (`open`, `reference`, `payload`, `sublayer`); population mask (optional); load payloads on/off
- **Targeting:** Primitive Location (mount location when loaded as reference or payload)
- **USD mapping:** `UsdStage::Open`, references, payloads, sublayers
- **Status:** **Partial.** Opens `.usda`, `.usdc` and `.usdz` composed (sublayers, references, payloads, inherits, specializes, variants), one primitive per mesh, curves or points prim, with materials, purposes and instancing; cameras, lights and skeletons are listed. **Modes:** Sublayer (the whole stage at its paths, written as a sublayer), Reference and Payload (the file's default prim, or a named one, at a Primitive Location, `/root` by default, its paths rebased; written as `references` or `payload` on that prim, with the axis and unit transform where the file's differ from the layer's). Not yet: `open` as its own mode, population mask, payload control, layer offsets.

### 2.2 Write USD (Save / Export Layer)
- **Purpose:** Write the authored result to disk.
- **Inputs:** scene
- **Parameters:** file path; format (`usda`, `usdc`, `usdz`); flatten on/off; target layer (root, session, specific sublayer)
- **Targeting:** none
- **USD mapping:** `Export`, `Flatten`, `Save`
- **Status:** **Partial.** Write USD writes an **override layer** (`.usda`): it sublayers each stage's file and holds only what changed (moved prims, changed points, topology and primvars, blocked attributes, pruned prims deactivated, material binding and purpose). Prims made in Imago are written as `def`s under `def Xform` parents, with a `defaultPrim`. Not yet: `usdc`/`usdz`, flatten, choosing the target layer, time samples over a frame range.

### 2.3 Sublayer
- **Purpose:** Add layers to the layer stack.
- **Inputs:** scene
- **Parameters:** ordered layer list; insert position; layer offset/scale
- **Targeting:** none
- **USD mapping:** `subLayerPaths`
- **Status:** **Read only** (sublayers are resolved on load).

### 2.4 Cache / Freeze
- **Purpose:** Bake an upstream result so it stops recomputing.
- **Inputs:** scene
- **Parameters:** cache on/off; invalidate
- **Targeting:** none
- **Status:** **Not yet** as an operator. Every operator's result is already kept until it or something above it changes (keyed by parameters and inputs), so a Freeze is only needed to hold a result through changes upstream, or to keep it on disk.

---

## 3. Prim Creation

All operators in this section take a **Primitive Location**. Parents on the path are created as Xforms.

### 3.1 Create Prim
- **Purpose:** Generic prim creation.
- **Parameters:** name; schema type; specifier (`def`, `over`, `class`); applied API schemas
- **USD mapping:** `DefinePrim`, `OverridePrim`, `CreateClassPrim`
- **Status:** **Not yet.**

### 3.2 Xform / Scope
- **Purpose:** Grouping prims (Xform has a transform, Scope does not).
- **Status:** **Partial.** Xforms are made implicitly for the parents of a Primitive Location. No operator authors one on its own, and there is no Scope.

### 3.3 Basic Primitives
Cube, Sphere, Cylinder, Cone, Capsule, Plane.
- **Parameters:** size/radius/height/axis; transform; subdivision hint
- **USD mapping:** `UsdGeomCube`, `UsdGeomSphere`, `UsdGeomCylinder`, `UsdGeomCone`, `UsdGeomCapsule`, `UsdGeomPlane`
- **Status:** **Partial.** Cube, Sphere and Grid (the Plane) exist as **shapes**: they make `Mesh` prims, so Edit Poly, the UV operators and ICE can edit them. Not yet: Cylinder, Cone, Capsule, and an option to author the implicit `UsdGeom` schemas instead of a mesh. Implicit shapes in files are not read yet.

### 3.4 Mesh / BasisCurves / Points
- **Purpose:** Author or import explicit geometry.
- **Parameters:** points; face counts/indices (mesh); curve basis/type/wrap (curves); widths; normals; subdivision scheme
- **USD mapping:** `UsdGeomMesh`, `UsdGeomBasisCurves`, `UsdGeomPoints`
- **Status:** **Partial.** All three are read with their primvars, kept as authored, shown in the Primitive Inspector (Point, Edge, Face, Corner, Object) and written back by Write USD. Explicit geometry comes from files and shapes; there is no operator that authors raw points or indices.

### 3.5 Camera
- **Parameters:** projection; focal length; horizontal/vertical aperture; clipping range; focus distance; f-stop; shutter open/close
- **USD mapping:** `UsdGeomCamera`
- **Status:** **Partial.** Cameras are primitives with no geometry (`Prim::camera`): a file's, read with every value above but the shutter, and the **Camera** operator's, at a Primitive Location with Translate and Rotate. Transform moves them; Write USD writes the operator's as `def Camera` and a file's moved camera as its transform; a pruned one is deactivated. The Viewer draws them and looks through any of them (its camera menu), showing the whole gate. Not yet: shutter, gate offsets and depth of field in the Viewer, animated cameras, editing a file camera's lens.

### 3.6 Lights
Rect, Disk, Sphere, Cylinder, Distant, Dome, Portal.
- **Parameters:** intensity; exposure; color/temperature; shaping (cone, focus); texture (dome); size
- **USD mapping:** `UsdLux*` schemas, `LightAPI`, `ShapingAPI`, `ShadowAPI`
- **Status:** **Read only** (listed by name and type).

### 3.7 Point Instancer
- **Parameters:** prototypes (relationship); proto indices; positions; orientations; scales; velocities; angular velocities; ids
- **USD mapping:** `UsdGeomPointInstancer`
- **Status:** **Read only.** Instancers and native instances are read and drawn by GPU instancing, and Transform moves their placements. Nested instancers (in another's prototype) are drawn through the outer one at each of its copies, down to four deep, and follow its points. Not yet: authoring them, editing their prototypes.

---

## 4. Hierarchy Editing

All operators in this section take **Primitive Locations**.

| Operator | Purpose | Key parameters | Status |
|---|---|---|---|
| **Merge** | Combine scenes (rule in 1.2) | Mode (Auto, Primitives, Flatten); later input stronger on the same path | **Done** (any number of inputs; the conflict rule not yet a parameter) |
| **Reparent / Graft** | Move prims under a new parent | destination path (Primitive Location); keep world transform | Not yet |
| **Rename** | Rename prims | find/replace; pattern; prefix/suffix | Not yet |
| **Duplicate** | Copy prims | count; naming pattern; per-copy transform | Not yet |
| **Prune / Delete** | Remove prims | Matches: Remove / Keep only (the invert) | **Done** (Prune Primitives; written as `active = false`) |
| **Isolate** | Keep only the selected subtree | include ancestors on/off | Covered by Prune set to Keep only |
| **Set Active**, **Set Kind**, **Set Purpose**, **Set Visibility** | Activate prims; assign kind, purpose, visibility | | Not separate operators: each is a **Property** row of Set Attribute (6.1): `active`, `kind`, `purpose`, `visibility`. **Done** through Set Attribute |

Imago operators not in the original list:

| Operator | Purpose | Status |
|---|---|---|
| **Pick Primitives** | Choose prims for the operators after it, by regular expression | **Done**, kept for older graphs; Primitive Locations replace it |
| **Unpack** | Every primitive as one plain mesh | **Done** |

---

## 5. Composition Arcs

All take **Primitive Locations** (targets) unless noted. Arc strength follows USD's LIVRPS ordering: Local, Inherits, VariantSets, References, Payloads, Specializes.

Status for the whole section: **Read only**, but for references and payloads to a file. Every arc is resolved when a file is loaded (what usdview shows, Imago shows). Load USD's Reference and Payload modes author one reference or payload to its file's prim (2.1); there is no operator yet adding an arc to a prim already in the scene.

| Operator | Purpose | Key parameters |
|---|---|---|
| **Reference** | Add reference arc | asset path; target prim; layer offset; list-edit op (prepend/append/explicit) |
| **Payload** | Add payload arc | asset path; target prim; load on/off |
| **Load / Unload Payload** | Control payload population | list; load rule |
| **Inherit** | Add inherit arc to a class | class path |
| **Specialize** | Add specialize arc | source path |
| **Variant Set** | Define a variant set and its variants | set name; variant names; per-variant edits (sub-network) |
| **Variant Select** | Choose a variant | set name; variant name |

> **Open question:** do list-edit operations (prepend/append/delete/explicit) need to be exposed on every arc operator, or only in an advanced tab? (A third tab next to Parameters and Prims Location would fit the Parameters pane as it is.)

---

## 6. Properties & Attributes

All take **Primitive Locations**.

**What the scene graph writes, and what ICE writes.** Scene-graph operators author light opinions: one value per primitive, which keep the override layer small. Values per point, per face or per corner are ICE's, once it is refactored: changing them rewrites the primitive's whole arrays, and Write USD writes them in full. Geometry operators (Edit Poly, the UV operators) still change topology, points and UVs, which is what they are for.

Attribute contexts in Imago map to USD interpolations: Object = `constant`, Point = `vertex`, Face = `uniform`, Corner = `faceVarying`. `varying` is read as Point and has no context of its own yet. Every Imago attribute is a primvar (`primvars:name`), apart from the built-in `points`, `normals` and `widths`.

### 6.1 Set Attribute
- **Purpose:** One value per primitive, for anything the scene graph sets on a prim: a primvar the renderer sees, or a property of the prim. The only "set" operator of the scene graph: it does what USD's Set Attribute, Set Primvar, Set Metadata, Set Active, Set Kind, Set Purpose, Set Visibility and Subdivision Scheme do.
- **Parameters:** a list of rows, one value each, so one operator can set several things on the same prims. Each row: context; name; value type; value; time (default value, or a sample at the current frame).
- **Context:**

  | Context | Written as | Use |
  |---|---|---|
  | **Object** | `primvars:name`, `interpolation = "constant"` | Data for rendering: shaders read it by name, and on a group it reaches every prim below that does not set its own (`displayColor`, a shader variation id) |
  | **Property** | the prim's own attribute or metadata, `name` as typed | What the prim is and how it is used: schema properties (`visibility`, `purpose`, `subdivisionScheme`, `doubleSided`, a camera's `focalLength`, a light's `intensity`), metadata (`active`, `kind`, `instanceable`), and data that is not for rendering (asset ids, `userProperties:*`). Not seen by shaders, not inherited, on any prim type |

- **Property presets:** the name field offers the common ones, each with its type and, where USD fixes them, its values:

  | Property | Values | Written as |
  |---|---|---|
  | `visibility` | `inherited`, `invisible` | attribute (`token`), may be time-sampled |
  | `purpose` | `default`, `render`, `proxy`, `guide` | attribute (`uniform token`) |
  | `active` | on, off | metadata |
  | `kind` | `component`, `subcomponent`, `group`, `assembly` | metadata |
  | `instanceable` | on, off | metadata |
  | `subdivisionScheme` | `catmullClark`, `loop`, `bilinear`, `none` | attribute (`uniform token`) on meshes: the subdivision the renderer does, as opposed to Edit Poly's Subdivide, which changes the mesh |
  | `doubleSided` | on, off | attribute (`uniform bool`) |

  Any other name can be typed, with its value type chosen.
- **Value types:** `bool`, `int`, `float`, `double`, `string`, `token`, `float2`/`float3`/`float4`, `color3f`, `asset`; arrays later.
- **USD mapping:** `UsdGeomPrimvarsAPI::CreatePrimvar` (Object); `UsdAttribute::Set` or `UsdPrim::SetMetadata` (Property, chosen by the name)
- **Notes:**
  - Object values show in the Primitive Inspector's Object tab, as Object attributes do. Property values show in its Properties tab, with where each comes from: the file, Imago, the Set Attribute operator that set it, or the group it is inherited from.
  - Both contexts reach prims without geometry (an Xform, a camera, a light, a material): their values are opinions in the stage's tree (`StageTree::opinions`), written by Write USD as `over`s on their path. The Xforms Imago makes above a shape's Primitive Location take values too: each primitive below carries them (`Prim::above`), and Write USD writes them once on the Xform.
  - Values per point, face or corner are not offered: they are ICE's (see above).
  - Arbitrary metadata beyond the presets (`customData`, `documentation`) can come later as more Property names.
- **Status:** **Partial.** Rows of Object and Property values, the presets above, every value type, on the prims its Prims Location tab names (any prim of a stage) or, with none, on every primitive coming in (or the picked ones). In Imago: an Object value is an Object attribute of the primitive's `Geo`; `purpose` is the primitive's purpose; `active` off takes the prim, and what is below it, out of the scene; `visibility` invisible hides it, and what is below it, in the Viewer. Write USD writes them all (`primvars:name` constant, attributes, `uniform` where the schema has it, metadata). Values on a group are inherited below it in Imago as in USD: visibility, active, purpose where a primitive has none, Object values where it has no attribute of that name (shown in the Primitive Inspector). Not yet: time samples (values are default values), arrays, `customData` and `documentation`, inherited Object values read by ICE.

### 6.2 Set Relationship
- **Parameters:** relationship name; target list; list-edit op
- **Status:** **Not yet** (material binding is the only relationship written, when it changed).

### 6.3 Transform
- **Parameters:** translate; rotate (with order); scale; pivot; xformOp stack order; reset xform stack
- **USD mapping:** `UsdGeomXformable` xformOps
- **Status:** **Partial.** Translate, Rotate (X then Y then Z) and Scale, on the prims its Prims Location tab lists (or every prim). Moves placements, not points; instanced copies move as matrices; clips move too. Written back as one `xformOp:transform` that keeps the prim following its parents; `!resetXformStack!` is read and kept. Not yet: rotation order, pivot, authoring separate xformOps.

### 6.4 Attribute Delete / Rename
- **Parameters:** attribute name pattern; new name
- **Status:** **Not yet** (attributes an operator drops are written as blocked, `= None`).

### 6.5 ICE (programmable operator)
- **Purpose:** Programmable operator for deformers and primvar manipulation: where values per point, face and corner are written.
- **Status:** **Partial.** The ICE operator holds an ICE graph that runs per primitive, in its own space, and writes attributes. A big refactor is planned.

---

## 7. Time & Animation

| Operator | Purpose | Key parameters | Status |
|---|---|---|---|
| **Time Shift / Retime** | Offset or scale time | offset; scale; hold/loop | Partial: Retime, Time Warp, Trim, Loop and Set Timecode exist **for clips**; not yet for USD time samples |
| **Set Time Samples** | Author samples | attribute; sample list | Not yet (Write USD writes values at the current frame) |
| **Resample** | Interpolate to a new sample set | rate; interpolation (held/linear) | Partial: Retime (Resample) for clips |
| **Motion Blur Attrs** | Velocity, acceleration, shutter | velocities attr; accelerations attr; shutter open/close | Not yet |

USD time samples are read at the stage's start time only. Skeletal animation, skinning and blend shapes from USD are not read yet. Imago's mocap operators (Split Characters, Retarget, Reach, Body Collide, Capture, Skin…) work on clips and stay outside this spec.

---

## 8. Collections & Linking

Status for the whole section: **Not yet.** Collections would also be the third kind of Primitive Locations entry (1.3).

### 8.1 Create Collection
- **Parameters:** name; include list; exclude list; expansion rule (`explicitOnly`, `expandPrims`, `expandPrimsAndProperties`)
- **USD mapping:** `UsdCollectionAPI`

### 8.2 Light Linking / Shadow Linking
- **Targeting:** Primitive Locations of the lights
- **Parameters:** include collection; exclude collection
- **USD mapping:** `collection:lightLink`, `collection:shadowLink` on `UsdLuxLightAPI`

### 8.3 Material Binding by Collection
- **Parameters:** collection; material; binding strength
- **USD mapping:** `UsdShadeMaterialBindingAPI::Bind(collection, ...)`

---

## 9. Materials

Materials are **read**: UsdPreviewSurface, OpenPBR and Standard Surface, with the texture behind each input, listed in Load USD's parameters and drawn in the Viewer (colour, emissive, opacity textures). Bindings stay with their prims and are written back when changed. The Material operator makes a UsdPreviewSurface (9.1); Material Binding puts materials in the scene and binds them (9.2).

### 9.1 Create Material
- A **compound operator**: contains other operators, and you can dive into the graph inside.
- Nested compounds are supported.
- **Outputs:** surface, displacement, volume
- **USD mapping:** `UsdShadeMaterial`, with a shading network underneath
- **Status:** **Partial.** The **Material** operator is the simple form: a UsdPreviewSurface with fixed inputs (diffuse and emissive colour, metallic, roughness, opacity, opacity threshold, IOR, clearcoat) and a texture file per input (diffuse, emissive, metallic, roughness, opacity, normal, occlusion). Its output is one material, named by the operator; viewed, it is drawn on a sphere. Written as a `Material` with a `UsdPreviewSurface`, a `UsdUVTexture` per file and a `UsdPrimvarReader_float2` for `st`. The compound form (a graph inside, nested; needs compounds, 1.4) puts out the same material.

### 9.2 Material Assign (Mat Bindings)
In Imago: **Material Binding**.
- **Inputs:** the scene, on top; one port per material, on the left, numbered (the materials feed into the operator)
- **Material Library Location:** where the plugged materials go in the scene, `/materials` by default, each by its operator's name. Two materials with one name are reported.
- **Parameters:** an ordered **binding list**. Each row is a numbered entry holding a location pattern, as in a Prims Location tab:

  ```
  1  /root/char/geo/head
  2  /root/char/geo/body
  3  /root/*/*/glass
  4  (empty row for the next entry)
  ```
- **Per-row options:** material: the input of the same number by default, any input with a material, or Location: a material already in the scene, by path, on a line under the row (typed, picked or dropped); strength (`strongerThanDescendants` / `weakerThanDescendants`); purpose (`allPurpose`, `preview`, `full`)
- **USD mapping:** `UsdShadeMaterialBindingAPI`
- **Status:** **Partial.** Material Binding: the scene on top, a material input per number on the left (one always free), the Material Library Location, and its rows on its Bindings tab (same matching rules, a count per row, drag and drop from the Scene Explorer), each with a Material column. The later row wins. Written as `def Material`s under Scopes and `material:binding` on each primitive. Not yet: a binding on a group kept as one inherited opinion, strength, purpose, collections.

> **Rule (recommended):** when two rows match the same prim, the **later row wins**, as the later input wins in Merge.

### 9.3 Material Variants
- Uses Variant Set / Variant Select (section 5) scoped to a material.
- **Status:** **Not yet.**

### 9.4 Shading Nodes
- MaterialX standard library nodes
- Studio OSL-backed nodes (wrapped as nodedefs)
- OpenPBR surface
- **Output nodes:** Surface, Displacement, Volume
- **Status:** **Not yet** (OpenPBR is read and shown).

---

## 10. Geometry Editing

All take **Primitive Locations** where they edit prims.

| Operator | Purpose | Key parameters | Status |
|---|---|---|---|
| **Edit Poly** | Polygon modelling in one operator | a stack of operations, each on its own selection | **Done**: Extrude, Bevel, Inset, Transform, Delete, Remove, Weld, Collapse, Connect, Cap, Bridge, Detach, Break, Flip, Make Planar, Relax, Tessellate, Subdivide, Chamfer, Extrude Vertex/Edge, Outline, Hinge, Slice, Insert Vertex, Triangulate, Turn. Several listed prims are edited merged as one for now |
| **Subdivide** | Subdivision/tessellation | levels; scheme | Partial: Edit Poly's Subdivide and Tessellate change the mesh. The subdivision the renderer does is Set Attribute's `subdivisionScheme` (6.1) |
| **Normals** | Compute or fix normals | angle threshold; weighting | Not yet (normals are read and kept, computed when missing) |
| **UV Project** | Generate UVs | projection type; axis; scale | Partial: UV Unwrap (Conformal/LSCM, Box, Planar; UDIM tiles), UV Transform, UV Edit. These still merge their input into one mesh and are to be moved to Primitive Locations |
| **Boolean** | Mesh booleans | operation; inputs A/B | Not yet |
| **Smooth** | Relax geometry | iterations; strength | Partial: Edit Poly's Relax |
| **Mesh to Points** / **Points to Mesh** | Convert representation | per-vertex or per-face | Not yet (ICE graphs can scatter and copy to points) |
| **Subdivision Scheme** | Set scheme on meshes | `catmullClark`, `loop`, `bilinear`, `none` | Not a separate operator: Set Attribute's `subdivisionScheme` (6.1), written for the renderer. The scheme is read and kept |
| **Crease / Corner** | Author crease and corner data | indices; sharpness | Not yet (creases and holes in files are kept, not shown) |

---

## 11. Rendering

Status for the whole section: **Not yet.** The plan is rray as Imago's render delegate, taking the scene plus its edits (see `ARCHITECTURE.md`).

### 11.1 Render Settings
- **Parameters:** resolution; pixel aspect ratio; camera (relationship); sampling settings; included purposes; motion blur
- **USD mapping:** `UsdRenderSettings`

### 11.2 Render Product
- **Parameters:** output path; product type; camera override; ordered render vars
- **USD mapping:** `UsdRenderProduct`

### 11.3 Render Var
- **Parameters:** AOV name; source type; source name; data type
- **USD mapping:** `UsdRenderVar`

### 11.4 Render
- **Purpose:** Trigger the render.
- **Parameters:** frame range; output mode (local / farm); denoise on/off
- **Inputs:** scene (must contain Render Settings)

---

## 12. Flow Utilities

| Operator | Purpose | Status |
|---|---|---|
| **Switch / Conditional** | Select an input by index or expression | Not yet |
| **Null / Dot** | Organisational pass-through | Not yet (a Placeholder with one input and one output passes through) |
| **Compound / Subgraph** | Encapsulate a network (section 1.4) | Not yet |
| **Python Script** | Run Python against the stage | Not yet |
| **Expression** | Drive parameter values | Not yet |
| **Placeholder** | An operator that does not exist yet: named inputs and outputs, data passed through, and a description of what it should do, saved with the graph | **Done** |
| **Bypass** (on every operator) | Hand the first input on unchanged | **Done** |
| **View flag** (on every operator) | Which operator's output is the scene; replaces an Output operator | **Done** |

---

## 13. Minimum Viable Operator Set

Enough for a complete load, edit, bind, render loop:

| # | Operator | Status |
|---|---|---|
| 1 | Load USD | Partial (Sublayer, Reference, Payload; no population mask) |
| 2 | Create Prim | Not yet (Cube, Sphere, Grid exist) |
| 3 | Merge | Done |
| 4 | Transform | Partial (no pivot or rotation order) |
| 5 | Set Attribute (Object, Property) | Partial (no time samples or arrays) |
| 6 | Prune | Done |
| 7 | Reference | Partial (Load USD's Reference and Payload modes) |
| 8 | Variant Select | Not yet (read only) |
| 9 | Material Assign | Partial (Material Binding: no strength, purpose or group bindings yet) |
| 10 | Light | Not yet (read only) |
| 11 | Camera | Partial (Camera operator, file cameras, looked through in the Viewer) |
| 12 | Render Settings | Not yet |
| 13 | Render | Not yet |
| 14 | Write USD (Save) | Partial (override layer in `usda`) |
