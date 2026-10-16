# MaterialX

Imago's shading graphs are MaterialX graphs: the **MaterialX** operator holds a graph of MaterialX nodes, by their MaterialX names, inputs and types, and puts out a material, as the Material operator does. Material Binding takes either. The Material operator stays as it is: one UsdPreviewSurface with fixed inputs, for quick work. The template "MaterialX: three shaders" shows three.

## The node library

MaterialX's own libraries are part of Imago, in the crate `crates/xms-matx`: MaterialX 1.39, copied unchanged from the MaterialX repository into `crates/xms-matx/libraries` (version, commit and licence in `libraries/SOURCE.md`) and compiled into the program, so nothing is installed beside it.

| Library | What it has |
|---|---|
| stdlib | the standard nodes: maths, conditionals, channels, textures (`image`, `tiledimage`), procedurals (`noise3d`, `checkerboard`), geometry (`texcoord`, `normal`, `position`), compositing, colour adjustment, `surfacematerial` |
| pbrlib | the closures shaders are made of: BSDFs, EDFs, VDFs, `surface`, `layer`, `mix` |
| bxdf | the surface shaders: `standard_surface`, `open_pbr_surface`, `UsdPreviewSurface` (and `UsdUVTexture`, `UsdPrimvarReader`), `gltf_pbr`, `disney_principled`, the Lama nodes, and the translation nodes between them |
| cmlib | colour space conversions |
| nprlib | non-photorealistic nodes (`gooch_shade`, ...) |
| lights | light shaders |

That is 824 node definitions of 226 nodes. A node has one definition per set of types it works on: `add` is `ND_add_float`, `ND_add_color3`, `ND_add_color3FA` (a colour plus a float) and more. In a graph it is one `add` node; which definition it is follows from what is connected to it (a colour into `in1` makes it `ND_add_color3`) and, when nothing decides, from what it feeds. With nothing connected it is its first definition. A node whose definitions are versioned uses its default version: `standard_surface` is 1.0.1.

Each input carries what the library says about it: its type, its default, whether it is uniform (set once per material, as a file name is), the geometric property read when nothing is connected (`UV0` for a texture coordinate), its choices (an address mode's `constant`, `clamp`, `periodic`, `mirror`), and its label, folder, limits and documentation as the library writes them. The MaterialX operator's parameters are laid out from these.

The crate also holds the graph itself (`Graph`: nodes, the values set on them and their connections, connecting with the definitions picked), the graph as a MaterialX document and back (`document`), the material as a USD MaterialX network (`usd`), and what the Viewer shows of it (`preview`). It has no interface code; its tests run on their own (`cargo test -p xms-matx`).

To move to a newer MaterialX: copy the same files over those in `crates/xms-matx/libraries`, change the version and commit in `SOURCE.md`, and run `cargo test -p xms-matx`.

## The MaterialX operator

In the add menu under Materials. A new one is a `standard_surface` wired into a `surfacematerial`. Its output is a material, purple, on its right side, as the Material operator's: wire it into a Material Binding, which puts it in the scene under its Material Library Location by the operator's name.

Double-click it, or select it and press ↓, to open its graph in the Node Graph pane (see [Interface](interface.md#materialx-graphs)). Inside:

- **Adding:** right-click or Tab. The surface shaders, `surfacematerial` and the image nodes come first; then every node of the library by group (Shading, Material, Texture 2D, Procedural 2D, Math, ...), or typed in the search line (Enter takes the first found).
- **Connecting:** drag from an output onto an input. When the types differ, the nodes become the definitions that take them: an `add` given a colour becomes `ND_add_color3`, a colour plus a float `ND_add_color3FA`; an `image` wired into a colour reads a colour, and a float `multiply` between them becomes a colour one too. When no definition takes the type the wire is refused and the canvas says why. Dropped on a node's body, the wire offers the inputs that can take it. A loop is refused.
- **Wrong wires:** a wire left between types that no longer join, after a node it reads was changed, is drawn red, and the Parameters pane of the operator counts them.
- **Taking off:** right-click a wire, or ✕ beside the input in the Parameters pane. Delete or Backspace deletes the selected node.

The **Parameters** pane shows the selected node: its name (letters, digits and `_`, unique in the graph), its definition, then its inputs grouped by the library's folders (Base, Specular, Coat, ... for `standard_surface`), each labelled by the library's label: a slider where the library gives limits (typed values may go past the soft ones), a swatch and three values for a colour, values side by side for vectors, a list for choices, a field with 📂 for file names, a check box for a boolean, text for matrices and arrays. A connected input shows the node it reads (`← image1`). An input set on the node has ↺ to put it back to its default. Inputs the library marks advanced show when "Advanced" is ticked. With no node selected, or the operator selected in the scene graph, the pane shows the material's name, its surface shader and what the Viewer shows of it.

The graph is saved in the graph file as the MaterialX document it is (`.mtlx` text, with each node's place as `xpos` and `ypos`), and goes with its operator when copied, pasted, undone and redone.

## In USD

Write USD writes a MaterialX operator's material as a MaterialX shading network, as usdMtlx makes from a `.mtlx` file and Hydra render delegates read:

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

Every node the material reads is a Shader under it, its `info:id` its definition, with the inputs set on it (values, or `.connect` to the shader they read) and its outputs. Nodes the material does not read are not written. A graph with nothing going into its `surfacematerial` is written as the grey the Viewer shows. The Primitive Inspector lists the surface shader's definition and what is set on it.

## In the Viewer

The Viewer draws a material as a colour, a roughness, a metalness, an opacity and an emission, each maybe from a texture. For a MaterialX graph these come from the shader feeding the `surfacematerial`, by its own inputs: `standard_surface` (base_color times base, specular_roughness, metalness, opacity, emission_color times emission, coat), `open_pbr_surface` (base_color times base_weight, specular_roughness, base_metalness, geometry_opacity, emission_color when emission_luminance is above 0, coat_weight), `UsdPreviewSurface` and `gltf_pbr`. An input fed by a graph shows the first texture file found up that graph (an `image`, `tiledimage`, `UsdUVTexture` or glTF image), white under it as USD shows a textured colour; what the graph does to it on the way is not shown. Another shader shows grey.

## Not done yet

- Reading MaterialX networks from USD files, and `.mtlx` files, into a MaterialX operator's graph
- Writing a graph as a `.mtlx` file on its own (the saved form is one already)
- The Viewer: the whole graph as a shader (procedurals, maths, layers); today the surface shader's values and the first texture behind each
- Node graphs of the libraries (nodes MaterialX defines as graphs) are written by their definition, as MaterialX expects; they are not opened or expanded
- Compounds in a MaterialX graph (MaterialX node graphs as nodes): not planned for now; reusable sets of materials will be scene-graph compounds
- Shader code for the nodes (MaterialX's `genglsl`, `genmsl`, ... folders are not copied)
- Copying and pasting nodes inside a MaterialX graph, undo steps named for what was done in it (an edit inside is one "MaterialX" step of the operator)
