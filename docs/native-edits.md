# Native edits (plan)

Written 9 November 2026, not yet agreed: a direction to take when rendering
in Imago needs it, not a decision. It follows
[arnold-native.md](arnold-native.md), where rendering in Imago is built.

## Where it stands

Rendering in Imago (the Viewer, the Image Viewer, `--headless --render`)
writes a USD layer of the scene and has Arnold's USD reader load it. Points
that moved (a deformer, animation) are the exception: the layer has the
file's, and Imago's go to Arnold in memory before the first bucket
(`live::lighter`, `Change::Points`). A deformed groom writes no points; but
anything else Imago computes (an attribute, new topology, geometry made in
Imago) is written into the layer each frame, on every farm machine.

## The aim

Rendering in Imago sends what Imago computed to the renderer as the
renderer's own data, through the edits it already takes (`xms_render`'s
`Change`), and keeps USD for what the files have: the layer says which files
make the scene and how they are put together, and stays small whatever the
scene holds. This is how Gaffer renders (a backend per renderer, built from
its own primitives), kept to what Imago changed.

## What does not change

- **Write USD, and every command-line render** (kick, husk, Karma, the
  Render Command), write the whole layer as today: it is the scene
  description, complete and USD as the specification has it, for any
  program to open. Native edits are for rendering inside Imago only.
- **A scene of assembly, layout, scatter and materials** (no geometry made
  or deformed in Imago) is already as light as one made in Solaris: its
  files sublayered or referenced, transforms and visibility as overs,
  scatters as PointInstancers (arrays as long as the instances), materials as
  MaterialX networks, render settings. The plan only matters for what Imago
  computes heavily: deformers, ICE, geometry made in Imago.

## Steps

Each step stands on its own, with tests against the fake libai (the
parameters set on each node), and a render of the same scene both ways
(the whole layer, and the light layer with native edits) giving the same
parameters.

1. **Attributes in memory.** `Change::Primvar { name, context, values }`,
   set on the node Arnold's USD reader made (named by its path) with
   `AiNodeSetArray`. Each renderer maps names its own way; Arnold's:
   `widths` to `radius` (half), `normals` to `nlist`/`nidxs`, `st` to
   `uvlist`, anything else a user parameter of its interpolation (constant,
   uniform, varying, indexed). `lighter` leaves such attributes as the file
   has them. A deformed groom with its widths changed then writes nothing
   heavy.
2. **Motion samples in memory.** The edits carry samples at the shutter's
   times (`deform_keys`, `transform_keys`): deforming hair with motion blur,
   still nothing written. Today blur comes from the layer's time samples.
3. **Topology in memory.** `Change::Geometry(geo)`, a node's whole geometry
   replaced: `nsides`, `vidxs`, `vlist` for a mesh, `num_points`, `points`
   for curves. An ICE operator adding curves, a remesh.
4. **Geometry made in Imago, built natively.** `Change::Create(prim)`: a
   `polymesh`, `curves` or `points` node named by its path, with its world
   matrix, visibility and material (bound to the material the layer has).
   The layer no longer holds them.
5. **What stays USD on purpose:** the stages as sublayers and references,
   prims taken away (`active = false`), the scene's axes and unit
   (`xformOp:transform:toScene`), material bindings and MaterialX networks
   (small, and costly to build again natively), render settings, and
   instancers (prototypes, per-instance primvars), until the steps above are
   solid.

## What it gives

- **Farms:** the layer is the same few kilobytes each frame; nothing heavy is
  written or read again per frame; Arnold reads the files once and Imago
  sends what it changed.
- **IPR:** every change Imago makes is an edit, not a layer written and
  loaded again.
- **What renders is what Imago has:** what Imago changed reaches the renderer
  as Imago holds it, not after USD composes it a second time; USD stays the
  authority for the files.
- **Other renderers:** a backend implements the same edits; the USD part is
  shared.

## What it costs

- **Two ways to the same picture** to keep in step: the whole layer and the
  native edits. The tests comparing them are the guard.
- **Each renderer's translation** (Arnold's radius is half a width, its user
  parameters' types, indexed normals): where the bugs would be.
- **Order:** an edit lands once Arnold's USD reader has made its nodes; points
  rely on it already, topology and new nodes more.

## Order to do it in

Steps 1 and 2 first (a deformed groom rendered on a farm), then 3 and 4.
