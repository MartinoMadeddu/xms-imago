# Rendering

Imago authors what a renderer needs to render the scene, as USD has it (UsdRender): a **RenderSettings** prim, a **RenderProduct** for each image, and a **RenderVar** for each AOV. Three operators make them, in the **Render** category of the add menu. Write USD writes them with the scene; **Render Command** then runs husk (or any command line) on that layer. Imago does not render inside itself yet.

```
Scene ─► RenderScene Settings ─► Render Settings ─► Render Variables ─► Write USD ─► Render Command
```

Each operator takes the scene on its one input and hands it on with its render prims added or changed, so they can sit anywhere in the graph. Render Settings and Render Variables do not need RenderScene Settings wired into them: they find the RenderSettings prim in the scene coming in.

## RenderScene Settings

What is rendered, whatever renders it. It makes the RenderSettings prim (`/Render/Settings` by default) and a RenderProduct (`/Render/Products/beauty`), under Scopes.

| Group | Parameter | USD |
|---|---|---|
| Primitive | Primitive Location | the RenderSettings prim's path; the layer names it (`renderSettingsPrimPath`) |
| Camera | Camera: a path, or picked from the cameras coming in. Empty: the first camera coming in | `rel camera` |
| Image | Resolution, with presets (HD 720, HD 1080, 2K DCI, UHD, 4K DCI, square) | `uniform int2 resolution` |
| | Pixel Aspect | `uniform float pixelAspectRatio` |
| | Aspect Conform: Expand Aperture, Crop Aperture, Adjust Aperture Width, Adjust Aperture Height, Adjust Pixel Aspect Ratio | `uniform token aspectRatioConformPolicy` |
| | Overscan: how much is rendered beyond the frame on every side, as a share of it (0.1 is 10%) | `uniform float4 dataWindowNDC = (-o, -o, 1 + o, 1 + o)` |
| Render | Motion Blur, Depth of Field | `uniform bool disableMotionBlur`, `disableDepthOfField` (off is `true`) |
| | Purposes: default, render, proxy, guide | `uniform token[] includedPurposes` |
| Output | Product: the RenderProduct's path | `rel products` |
| | Output Image (`render/beauty.$F4.exr`): frame tokens are the renderer's to expand, as husk does | `token productName`, `uniform token productType = "raster"` |

Filtering and bit depth are not here: they belong to each RenderVar, or are the renderer's own settings.

## Render Settings

How a renderer renders it. Choose the **Renderer** from its menu: **Karma**, **Arnold**, any renderer with a definition file, or **Custom**.

**Which RenderSettings prim:** the one the scene names (made by RenderScene Settings), else the first one made in Imago, else the first one of a loaded file (changed with an `over`), else it makes one at its own Primitive Location (`/Render/Settings`). The panel says which.

**Settings tab:** the renderer's settings, by group (Sampling, Limits, Ray Depth, System…), each with its label, its range and a tooltip with the attribute's name. They are written as namespaced attributes on the RenderSettings prim: `karma:global:bucketsize`, `arnold:global:AA_samples`. Every setting is written, changed or not; a changed one has a dot after its label, and **Reset to defaults** puts them all back. Each renderer keeps its own values: switching from Arnold to Karma and back finds Arnold's again.

**Extra Settings:** settings the definition does not list, as rows of name, type and value, written under the renderer's namespace (`arnold:global:` + the name) unless the name has one of its own (`karma:object:geosamples` stays as typed).

**Custom:** type the renderer's name (`rray`). Its settings are rows you make, each written under that name: `spp` (int 64) is `int rray:spp = 64`, `bounce` is `rray:bounce`. Until it has a name nothing is written; the rows stay as you type or change it.

**AOVs tab:** the renderer's standard AOVs as checkboxes, by group (Beauty, Light paths, Surface, Geometry, IDs); the beauty (`C` for Karma, `RGBA` for Arnold) is ticked to start with. **All**, **None** and **Defaults** tick them together. Each ticked AOV becomes a RenderVar beside the product (`/Render/Products/Vars/<name>`) with its `sourceName`, `sourceType`, `dataType` and its name in the image (`driver:parameters:aov:name`, which husk needs on every RenderVar), added to the product's `orderedVars`. When there is no product yet, `Products/beauty` is made beside the RenderSettings prim. A Custom renderer has no standard AOVs: add them with Render Variables.

### Renderer definitions

A renderer is a JSON file: its name, label and namespace, its settings and its standard AOVs. Karma's and Arnold's come with Imago (`assets/renderers/karma.json`, `arnold.json`). More go in the **renderers folder**: `$XMS_RENDERERS`, else `renderers` in Imago's config folder (`~/.config/xms/renderers` on Linux, `%APPDATA%\xms\renderers` on Windows). A file there adds a renderer to the menu, or replaces the one of the same name. Files that do not read are listed in the panel, with why.

```json
{
  "name": "rray",
  "label": "rray",
  "namespace": "rray",
  "about": "One line on the renderer, shown in the panel.",
  "params": [
    { "name": "rray:spp", "type": "int", "default": 64, "min": 1, "max": 65536,
      "group": "Sampling", "label": "Samples", "help": "Samples per pixel." },
    { "name": "rray:integrator", "type": "token", "default": "path", "choices": ["path", "bdpt"],
      "group": "Sampling", "label": "Integrator" }
  ],
  "aovs": [
    { "name": "beauty", "sourceName": "color", "sourceType": "raw", "dataType": "color4f",
      "group": "Beauty", "label": "Beauty", "default": true }
  ]
}
```

Types: `bool`, `int`, `float`, `double`, `string`, `token`, `asset`, `float2`, `float3`, `float4`, `color3f`. A setting with `"unconfirmed": true` says so in its tooltip: its name follows the renderer's documentation as far as Imago knows, but was not checked against the renderer.

An AOV can also give its RenderVar's `format` (`driver:parameters:aov:format`: `color4h`, `float3`), `multiSampled`, and `extras`; `var_defaults` lists attributes every RenderVar of the renderer carries. Karma's carry `clearValue` and `karma:filter` (`["ubox",{}]`), as Houdini writes them.

**Karma's AOVs** follow what Houdini 20.5's Karma Render Settings writes: the beauty `C` is the light path expression `C.*[LO]` (color4f, written as `color4h`), the light path AOVs `color3h`, P `ray:hitP` and P (Camera Space) `ray:hitP_camera` as `float3`, UV the `st` primvar. Those not seen in a Houdini file are marked unconfirmed in their tooltip.

**Karma:** the names follow Houdini's Karma Render Settings. `karma:global:cacheratio`, `karma:object:geosamples` and `karma:object:dicingquality` are in SideFX's documentation; the others are marked unconfirmed. `karma --all-properties` lists every one with its type and default: correct the file from it, or put a corrected copy in the renderers folder.

**Arnold:** arnold-usd reads Arnold's options from `arnold:global:` attributes on the RenderSettings prim, each with the option's own name (`AA_samples`, `GI_diffuse_depth`, `enable_adaptive_sampling`).

## Render Variables

RenderVars of a product, set or overridden row by row.

- **Product:** a path, or picked from the products coming in. Empty: the RenderSettings prim's first product, else the first coming in, else `/Render/Products/beauty` (made, and added to the scene's RenderSettings prim).
- **Replace:** on, the product holds these RenderVars alone, in this order. Off, they are added to its own; a row named as one it has replaces it.
- **Rows:** one RenderVar each, at `<product's parent>/Vars/<name>`, its name written as `driver:parameters:aov:name` too (an extra of that name overrides it):
  - **On**: off, the RenderVar is not written and is taken off the product
  - **Name**: the prim, and the AOV's name in the image
  - **Source Name**: an AOV name for raw, a light path expression for lpe (`C<RD>.*L`), a primvar's name. Empty: the name
  - **Source Type**: raw, lpe, primvar, intrinsic
  - **Data Type**: color4f, color3f, float, normal3f, point3f, vector3f…
  - **Extras**: what the renderer reads on the RenderVar beyond those, picked or typed: `driver:parameters:aov:format` (Karma's channel format: half, float…), `driver:parameters:aov:name`, `arnold:filter`, `arnold:width`
- **+ From a renderer** fills a row from any renderer's standard AOVs.

## Render Command

Runs a command line on the system: husk on the layer the Write USD above it writes, say. Put it after that Write USD:

```
… ─► Render Settings ─► Write USD ─► Render Command
```

It runs **only when Run is pressed**, never while the graph cooks; on cook it hands on what comes in.

- **Scene:** what `$USD` is, the file of the nearest Write USD above it. **Write First** (on) writes that layer before running, so the render sees the scene as it is now; if it cannot be written, nothing runs and the panel says why. **Make Output Folders** (on) makes the folders the scene's RenderProducts write into before running (`render/` for `render/beauty.$F4.exr`, next to the layer as husk takes it): renderers do not make them.
- **Command:** the command line, with **Presets** to start from: husk with Karma CPU, Karma XPU, Arnold (arnold-usd's `HdArnoldRendererPlugin`), one husk per frame, and an `echo` to try the tokens. The line under it shows it with its tokens put in. husk renders the RenderSettings prim the layer names, so the presets do not give one (add `-s /Render/Settings` to choose).
- **Tokens:** `$USD` the layer, `$NAME` the operator's name, `$F` the frame, `$F4` the frame in four digits (`$F2` to `$F9` likewise), `$FSTART`, `$FEND` and `$N` the range and how many frames; `${USD}` works too, and `$$` is a `$`. Any other `$` is left to the shell (`$HOME`). Put quotes around `"$USD"` for paths with spaces.
- **Frames:** the scene range, or a custom one. **Run Per Frame** runs the line once per frame, `$F` that frame, one after the other; off, it runs once for the range (husk renders ranges itself).
- **Environment:** the folder it runs in (empty: the layer's folder), and variables on top of Imago's (`HOUDINI_PATH`, `ARNOLD_PLUGIN_PATH`, licences). The program has to be on the `PATH` Imago was started with, or named by its full path.
- **Run:** Run, Stop and Clear, above its tabs; what it is doing, how long it has taken, and its progress, read from the percentages it prints (husk's `-V 2a` prints them).
- **Console tab:** what it prints, as a terminal shows it: light text on black, the command lines run in green, lines with errors or warnings in amber; the last 400 lines, Copy takes them. Pressing Run opens it; **Parameters** is the other tab. A Console pane for every run can follow.

It runs through the system's shell (`sh -c`; `cmd /C` on Windows), in the background: Imago stays in use. Stop, or closing Imago, ends the command and everything it started.

The layer holds the scene at the current frame (animation is not written yet), so every frame of a range renders the same picture for now.

## In the scene

The render prims are primitives with no geometry, as cameras and materials are: the Scene Explorer lists them (⚙ RenderSettings, 🖼 RenderProduct, 📑 RenderVar) under their Scopes, and the Primitive Inspector's Properties tab lists their attributes and relationships, with the render operator they came from. The Viewer does not draw them.

Write USD writes them as `def RenderSettings`, `def RenderProduct` and `def RenderVar` under `def Scope` parents, and names the RenderSettings prim in the layer's header (`renderSettingsPrimPath`). One that came from a loaded file is written as an `over` with what changed; its relationships are appended (`append rel orderedVars = [...]`), since Imago does not read the file's own targets yet.

```usda
#usda 1.0
(
    renderSettingsPrimPath = "/Render/Settings"
)

def Scope "Render"
{
    def Scope "Products"
    {
        def Scope "Vars"
        {
            def RenderVar "C"
            {
                uniform token dataType = "color4f"
                int driver:parameters:aov:clearValue = 0
                token driver:parameters:aov:format = "color4h"
                string driver:parameters:aov:karma:filter = "[\"ubox\",{}]"
                bool driver:parameters:aov:multiSampled = true
                string driver:parameters:aov:name = "C"
                uniform string sourceName = "C.*[LO]"
                uniform token sourceType = "lpe"
            }
        }

        def RenderProduct "beauty"
        {
            token productName = "render/beauty.$F4.exr"
            uniform token productType = "raster"
            rel orderedVars = </Render/Products/Vars/C>
        }
    }

    def RenderSettings "Settings"
    {
        uniform token aspectRatioConformPolicy = "expandAperture"
        uniform float4 dataWindowNDC = (0, 0, 1, 1)
        uniform bool disableDepthOfField = false
        uniform bool disableMotionBlur = false
        uniform token[] includedPurposes = ["default", "render"]
        int karma:global:pathtracedsamples = 1
        int karma:global:samplesperpixel = 9
        uniform float pixelAspectRatio = 1
        uniform int2 resolution = (1920, 1080)
        rel camera = </camera>
        rel products = </Render/Products/beauty>
    }
}
```

Render the layer with husk and Karma, or with Arnold through arnold-usd (husk or kick): the renderer reads the RenderSettings prim the layer names.

## Not done yet

- Rendering inside Imago (the Viewer's Render mode, rray as its render delegate), and the image streamed into the Viewer while husk renders
- Several Render Commands queued, or sent to a farm
- A file's own `renderSettingsPrimPath` read (its first RenderSettings prim is taken), a file's products and RenderVars read as relationships
- Several RenderSettings prims to choose between, a camera or resolution per product
- Settings that change over time
