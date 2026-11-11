# MaterialX

Imago's shading graphs are MaterialX graphs: the **MaterialX** operator holds a
graph of MaterialX nodes, by their MaterialX names, inputs and types, and puts
out a material, as the Material operator does. Material Binding takes either.
The Material operator stays as it is: one UsdPreviewSurface with fixed inputs,
for quick work. The template "MaterialX: three shaders" shows three.

## The node library

MaterialX's own libraries are part of Imago, in the crate `crates/xms-matx`:
MaterialX 1.39, copied unchanged from the MaterialX repository into
`crates/xms-matx/libraries` (version, commit and licence in
`libraries/SOURCE.md`) and compiled into the program, so nothing is installed
beside it.

| Library | What it has |
|---|---|
| stdlib | the standard nodes: maths, conditionals, channels, textures (`image`, `tiledimage`), procedurals (`noise3d`, `checkerboard`), geometry (`texcoord`, `normal`, `position`), compositing, colour adjustment, `surfacematerial` |
| pbrlib | the closures shaders are made of: BSDFs, EDFs, VDFs, `surface`, `layer`, `mix` |
| bxdf | the surface shaders: `standard_surface`, `open_pbr_surface`, `UsdPreviewSurface` (and `UsdUVTexture`, `UsdPrimvarReader`), `gltf_pbr`, `disney_principled`, the Lama nodes, and the translation nodes between them |
| cmlib | colour space conversions |
| nprlib | non-photorealistic nodes (`gooch_shade`, ...) |
| lights | light shaders |

MaterialX's libraries are 824 node definitions of 226 nodes. A node has one definition per set of
types it works on: `add` is `ND_add_float`, `ND_add_color3`, `ND_add_color3FA`
(a colour plus a float) and more. In a graph it is one `add` node; which
definition it is follows from what is connected to it (a colour into `in1` makes
it `ND_add_color3`) and, when nothing decides, from what it feeds. With nothing
connected it is its first definition. A node whose definitions are versioned
uses its default version: `standard_surface` is 1.0.1.

Each input carries what the library says about it: its type, its default,
whether it is uniform (set once per material, as a file name is), the geometric
property read when nothing is connected (`UV0` for a texture coordinate), its
choices (an address mode's `constant`, `clamp`, `periodic`, `mirror`), and its
label, folder, limits and documentation as the library writes them. The
MaterialX operator's parameters are laid out from these.

The crate also holds the graph itself (`Graph`: nodes, the values set on them
and their connections, connecting with the definitions picked), the graph as a
MaterialX document and back (`document`), the material as a USD MaterialX
network (`usd`), and what the Viewer shows of it (`preview`). It has no
interface code; its tests run on their own (`cargo test -p xms-matx`).

To move to a newer MaterialX: copy the same files over those in
`crates/xms-matx/libraries`, change the version and commit in `SOURCE.md`, and
run `cargo test -p xms-matx`.

## The MaterialX operator

In the add menu under Materials. A new one is a `standard_surface` wired into a
`surfacematerial`. Its output is a material, purple, on its right side, as the
Material operator's: wire it into a Material Binding, which puts it in the scene
under its Material Library Location by the operator's name.

Double-click it, or select it and press ↓, to open its graph in the Node Graph
pane (see [Interface](interface.md#materialx-graphs)). Inside:

- **Adding:** right-click or Tab. The surface shaders, `surfacematerial` and the
  image nodes come first; then every node of the library by group (Shading,
  Material, Texture 2D, Procedural 2D, Math, ...), or typed in the search line:
  ↓ and ↑ move the highlight through what is found, Enter adds the highlighted
  node, as in the scene graph's add menu.
- **Connecting:** drag from an output onto an input. When the types differ, the
  nodes become the definitions that take them: an `add` given a colour becomes
  `ND_add_color3`, a colour plus a float `ND_add_color3FA`; an `image` wired
  into a colour reads a colour, and a float `multiply` between them becomes a
  colour one too. When no definition takes the type the wire is refused and the
  canvas says why. Dropped on a node's body, the wire offers the inputs that can
  take it. A loop is refused.
- **Wrong wires:** a wire left between types that no longer join, after a node
  it reads was changed, is drawn red, and the Parameters pane of the operator
  counts them.
- **Taking off:** click a wire to select it (it shows in the selection colour)
  and press Delete or Backspace; or right-click it; or ✕ beside the input in the
  Parameters pane. With no wire selected, Delete or Backspace deletes the
  selected node.
- **Into and out of wires:** drag a node over a wire with Shift held (the wire
  lights) and let go: what fed the wire goes into its first free input that
  takes it, its first output that fits goes where the wire went, each node
  made the definition the types need. Drag a node with Shift held and past a
  few pixels it comes out of its wires: an input it fed is fed by what fed its
  first input of the same type, so the chain closes up.
- **Colours:** a node is coloured by its kind, the same whatever types it takes:
  shaders purple, textures blue, procedurals orange, geometry teal, maths,
  channels and the rest a neutral grey, the `surfacematerial` dark grey. Its
  ports and wires are coloured by type, as everywhere in XMS (see [Data type
  colours](interface.md#data-type-colours)): floats teal-green, integers and
  booleans blue, colours orange, vectors yellow, 4-vectors dark yellow, matrices
  light purple, strings and file names red-beige, closures pink, shaders and the
  material purple.

The **Parameters** pane shows the selected node: its name (letters, digits and
`_`, unique in the graph), its definition, then its inputs grouped by the
library's folders (Base, Specular, Coat, ... for `standard_surface`), each
labelled by the library's label: a slider where the library gives limits (typed
values may go past the soft ones), a swatch and three values for a colour,
values side by side for vectors, a list for choices, a field with 📂 for file
names, a check box for a boolean, text for matrices and arrays. A connected
input shows the node it reads (`← image1`). An input set on the node has ↺ to
put it back to its default. Inputs the library marks advanced show when
"Advanced" is ticked. With no node selected, or the operator selected in the
scene graph, the pane shows the material's name, its surface shader and what the
Viewer shows of it.

The graph is saved in the graph file as the MaterialX document it is (`.mtlx`
text, with each node's place as `xpos` and `ypos`), and goes with its operator
when copied, pasted, undone and redone.

## Renderers' shaders

Some shading MaterialX has no node for: a volume read from its VDB grids by
name is one. For those, a renderer's own shaders are nodes too, named with the
renderer's prefix and listed under the renderer in the add menu, each group of
its own below (**Arnold > Volume**). Only that renderer renders them; a `.mtlx`
saved with one opens in Imago, not in other MaterialX tools.

- **Shipped with Imago:** the few its renderer definitions ship, always there.
  Arnold's: `ai_standard_volume` (`assets/renderers/arnold_shaders.mtlx`,
  written for Imago from what `kick -info standard_volume` reports of Arnold
  7.5, not copied from Arnold's files): a volume's density, scattering,
  anisotropy, transparency and emission (none, a grid, the density, or black
  body from the temperature), each read from a grid by name (Density Channel
  `density`, Temperature Channel `temperature`, Emission Channel `heat`).
- **From the SDK:** with **Load Shader Definitions from SDK** on (File >
  Settings > Renderers > Shaders, saved with the graph), every shader of the
  renderer's own MaterialX definitions is offered too, read from its SDK
  (Arnold's `$ARNOLD_PATH/materialx/arnold/nodedefs.mtlx`: 140 nodes, among
  them `standard_surface`, `standard_volume`, `image`, its maths), each named
  with the prefix (`ai_standard_surface`, apart from MaterialX's
  `standard_surface`), its definition `ND_ai_…`. They replace the shipped ones
  of the same name. `XMS_RENDERER_SHADERS=1` in the environment turns it on
  for every graph, as a wrapper launching Imago would. Settings lists what was
  read, and any definition left out (a type Imago does not know). Read the
  first time it is turned on; turned off, the SDK's nodes are no longer
  offered, those in graphs stay.
- **A node whose definition is not loaded** (a graph made with the SDK's
  shaders, opened where they are off or the renderer is not installed) is
  kept: its values and wires, written to the graph file and to USD as they
  were read; its parameters say why they cannot be edited.
- **A choice** (Arnold's Interpolation, Emission Mode) is read by its name or
  its number, whichever the definition takes: Arnold's own definitions take
  numbers, Imago's shipped ones some names.

Wire `ai_standard_volume` into a `volumematerial` (Material, beside
`surfacematerial`), and bind the material to a volume (Load VDB) with Material
Binding. A graph may hold a `surfacematerial` and a `volumematerial`: one
material, with a surface and a volume.

A renderer definition says how (its `shaders` block, see [renderer
definitions](renderer-definitions.md)):

```json
"shaders": { "prefix": "ai_", "context": "arnold", "shipped": "arnold_shaders.mtlx",
             "sdk": "$ARNOLD_PATH/materialx/arnold/nodedefs.mtlx" }
```

## MaterialX files

**File > Import > MaterialX...** reads a `.mtlx` file and adds a MaterialX
operator for each of its materials, named after it, right of the nodes there.
**Load .mtlx...**, in a MaterialX operator's parameters, replaces its graph with
a material of a file: the one of the operator's name, else the first. **Save
.mtlx...** writes its graph as a MaterialX document (every node with its
definition, the values set on it and its wires, and where it is drawn), which
any MaterialX tool reads.

A file can come from anywhere: Substance, Houdini, Maya, the MaterialX examples.
What it holds is read as follows:

- **Materials** are its `surfacematerial` nodes, each with every node it reads.
  A file with none gives a material for each surface shader nothing reads, put
  into a `surfacematerial` of its name.
- **Node graphs** (`nodegraph`) are laid out flat: their nodes go into the graph
  with the rest (renamed only where a name is taken), a wire into a node graph's
  output goes to the node behind it, and an input reading the graph's interface
  takes what the interface is set to: its value, or the node it reads.
- **File names** take the `fileprefix` in force where they are written (the
  node's, its node graph's, the document's), and, relative, the file's folder:
  textures are found wherever the file is.
- **Included files** (`xi:include`) are read as part of it, from its folder.
- **Older documents:** a definition the library has under another name (1.38's
  `ND_normalmap`, now `ND_normalmap_float`) is found by the node and its
  types, as when a node names none.
- **Laid out** right to left, the material in the right column, each node a
  column left of the leftmost node it feeds.
- **Not read**, and said so in the message: looks and material assignments (bind
  the materials with Material Binding); nodes the file defines itself (a
  `nodedef` implemented by a `nodegraph` in the file, as the hair example does);
  an input a definition does not have, or a value not of its type, left at its
  default.

Every example material of MaterialX 1.39 (50 files, 65 materials) is read whole
by the tests, every wire between types that join: the files are in
`crates/xms-matx/tests/examples`.

## From a USD stage

Load USD reads the materials of its stage as MaterialX graphs too, when every
shader they read is a MaterialX one (an `info:id` such as
`ND_standard_surface_surfaceshader`), a renderer's Imago has a node for
(`arnold:standard_volume`, read as `ai_standard_volume`, its volume into a
`volumematerial`), or of the UsdPreviewSurface family
(`UsdPreviewSurface`, `UsdUVTexture`, `UsdPrimvarReader_*`, `UsdTransform2d`,
read as MaterialX's definitions of them). In Load USD's parameters each such
material has **→ MaterialX**: a MaterialX operator with its network, named after
it, to edit. Wired into a Material Binding whose Material Library Location is
the material's parent (`/root/Looks` for `/root/Looks/wood`), it takes the place
of the stage's material, at the same path, on the prims bound to it. A material
with another kind of shader (Arnold's, RenderMan's) says why it is not offered.

## In USD

Write USD writes a MaterialX operator's material as a MaterialX shading network,
as usdMtlx makes from a `.mtlx` file and Hydra render delegates read:

```
def Material "Gold"
{
    token outputs:mtlx:surface.connect = </root/materials/Gold/standard_surface1.outputs:out>

    def Shader "standard_surface1"
    {
        uniform token info:id = "ND_standard_surface_surfaceshader"
        color3f inputs:base_color = (1, 0.766, 0.336)
        float inputs:metalness = 1
        float inputs:specular_roughness = 0.25
        token outputs:out
    }
}
```

Every node the material reads is a Shader under it, its `info:id` its
definition, with the inputs set on it (values, or `.connect` to the shader they
read) and its outputs. A `volumematerial` gives the Material its
`outputs:mtlx:volume`, beside the surface of a `surfacematerial` in the same
graph.

A renderer's shaders are written as its USD reader reads them, as Solaris
writes them: `uniform token info:id = "arnold:standard_volume"` (the
definition's `context` and the shader's own name), the renderer's parameter
names as inputs, and the Material's terminal in the renderer's render context,
which other renderers leave alone:

```
def Material "smoke"
{
    token outputs:arnold:volume.connect = </materials/smoke/ai_standard_volume1.outputs:out>

    def Shader "ai_standard_volume1"
    {
        uniform token info:id = "arnold:standard_volume"
        float inputs:density = 2.5
        uniform string inputs:density_channel = "density"
        token outputs:out
    }
}
```

The UsdPreviewSurface family is written as USD has it, so every USD tool reads
it: `info:id = "UsdPreviewSurface"`, `"UsdUVTexture"`,
`"UsdPrimvarReader_float2"`, `"UsdTransform2d"`; their outputs `surface` and
`result` where MaterialX says `out`; tokens for their string inputs (`wrapS`,
with MaterialX's `periodic` written as USD's `repeat`), `normal3f` for the
normal; and a material whose surface is a UsdPreviewSurface connected by
`outputs:surface`, not `outputs:mtlx:surface`. Nodes the material does not read
are not written. A graph with nothing going into its `surfacematerial` is
written as the grey the Viewer shows. The Primitive Inspector lists the surface
shader's definition and what is set on it.

## In the Viewer

The Viewer draws a material as a colour, a roughness, a metalness, an opacity
and an emission, each maybe from a texture. For a MaterialX graph these come
from the shader feeding the `surfacematerial`, by its own inputs:
`standard_surface` (base_color times base, specular_roughness, metalness,
opacity, emission_color times emission, coat), `open_pbr_surface` (base_color
times base_weight, specular_roughness, base_metalness, geometry_opacity,
emission_color when emission_luminance is above 0, coat_weight),
`UsdPreviewSurface` and `gltf_pbr`. An input fed by a graph shows the first
texture file found up that graph (an `image`, `tiledimage`, `UsdUVTexture` or
glTF image), white under it as USD shows a textured colour; what the graph does
to it on the way is not shown. Another shader shows grey.

## Not done yet

- Nodes a `.mtlx` file defines itself (its own `nodedef`s and their node graphs)
- A `.mtlx` file's `volumematerial`s on import (its `surfacematerial`s are
  read)
- Emission Scaling's choices in the shipped `ai_standard_volume` (text, `none`
  by default; the SDK's definition has them)
- A node whose definition is not loaded is drawn with the ports its graph was
  read with, and its parameters are not editable
- Node graph interfaces kept as such: their inputs are laid out flat into the
  nodes that read them
- Looks and material assignments of `.mtlx` files
- A stage's material edited where it is (`→ MaterialX` makes a copy to bind in
  its place)
- Shader inputs USD has and MaterialX's definition does not (`UsdUVTexture`'s
  `sourceColorSpace`): left out, and said so
- The Viewer: the whole graph as a shader (procedurals, maths, layers); today
  the surface shader's values and the first texture behind each
- Node graphs of the libraries (nodes MaterialX defines as graphs) are written
  by their definition, as MaterialX expects; they are not opened or expanded
- Compounds in a MaterialX graph (MaterialX node graphs as nodes): not planned
  for now; reusable sets of materials will be scene-graph compounds
- Shader code for the nodes (MaterialX's `genglsl`, `genmsl`, ... folders are
  not copied)
- Copying and pasting nodes inside a MaterialX graph, undo steps named for what
  was done in it (an edit inside is one "MaterialX" step of the operator)
