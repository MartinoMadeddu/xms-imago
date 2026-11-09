# Rendering

Imago authors what a renderer needs to render the scene, as USD has it
(UsdRender): a **RenderSettings** prim, a **RenderProduct** for each image, and
a **RenderVar** for each AOV. Three operators make them, in the **Render**
category of the add menu. Write USD writes them with the scene; **Render
Command** then runs husk (or any command line) on that layer. Imago does not
render inside itself yet.

```
Scene ─► Render Commons ─► Render Settings ─► Render Variables ─► Write USD ─► Render Command
```

Each operator takes the scene on its one input and hands it on with its render
prims added or changed, so they can sit anywhere in the graph. Render Settings
and Render Variables do not need Render Commons wired into them: they find the
RenderSettings prim in the scene coming in.

## Render Commons

What is rendered, whatever renders it. It makes the RenderSettings prim
(`/Render/Settings` by default) and a RenderProduct (`/Render/Products/beauty`),
under Scopes.

| Group | Parameter | USD |
|---|---|---|
| Primitive | Primitive Location: a new prim's path, or one of the RenderSettings coming in (**Pick**) | the RenderSettings prim rendered: the layer names it (`renderSettingsPrimPath`) |
| Camera | Camera: a path, or picked from the cameras coming in. Empty: the first camera coming in | `rel camera` |
| Image | Resolution, with presets (HD 720, HD 1080, 2K DCI, UHD, 4K DCI, square) | `uniform int2 resolution` |
| | Pixel Aspect | `uniform float pixelAspectRatio` |
| | Aspect Conform: Expand Aperture, Crop Aperture, Adjust Aperture Width, Adjust Aperture Height, Adjust Pixel Aspect Ratio | `uniform token aspectRatioConformPolicy` |
| | Overscan: how much is rendered beyond the frame on every side, as a share of it (0.1 is 10%) | `uniform float4 dataWindowNDC = (-o, -o, 1 + o, 1 + o)` |
| Render | Motion Blur, Depth of Field | `uniform bool disableMotionBlur`, `disableDepthOfField` (off is `true`) |
| | Purposes: default, render, proxy, guide | `uniform token[] includedPurposes` |
| Output | Product: the RenderProduct's path | `rel products` |
| | Output Image (`render/beauty.$F4.exr`, `$JOB/render/$GNAME.$F4.exr`): Imago's variables are put in, the frame's (`$F4`) left for the renderer, as husk expands them | `token productName`, `uniform token productType = "raster"` |

Filtering and bit depth are not here: they belong to each RenderVar, or are the
renderer's own settings.

**Which RenderSettings is rendered.** One prim, decided in one place:

- With a Render Commons, the prim at its Primitive Location: a new one, or one
  of a loaded file's, picked from those coming in. A file's is written as an
  `over` with Render Commons' values over its own, and named. The panel lists
  the other RenderSettings in the scene, which are not rendered.
- Without one, the prim the loaded file names (its `renderSettingsPrimPath`),
  else the first one made in Imago. Write USD names it again in its layer,
  since a renderer reads that from the layer it opens: a Houdini scene renders
  with its own settings.
- To keep a file's settings and change a few, edit it with Edit Primitive
  instead (its RenderSettings type): only what is changed is written.

The Scene Explorer marks the one rendered (`RenderSettings · rendered`).

## Render Settings

How a renderer renders it. Choose the **Renderer** from its menu: **Karma**,
**Arnold**, any renderer with a definition file, or **Custom**.

**Which RenderSettings prim:** the one the scene names (Render Commons'),
else the first one made in Imago, else the one a loaded file names, else its
first (changed with an `over`), else it makes one at its own Primitive Location
(`/Render/Settings`). The panel says which.

**Settings tab:** the renderer's settings, by group (Sampling, Limits, Ray
Depth, System…), each with its label, its range and a tooltip with the
attribute's name. They are written as namespaced attributes on the
RenderSettings prim: `karma:global:bucketsize`, `arnold:global:AA_samples`.
Every setting is written, changed or not; a changed one has a dot after its
label, and **Reset to defaults** puts them all back. Each renderer keeps its own
values: switching from Arnold to Karma and back finds Arnold's again.

**Extra Settings:** settings the definition does not list, as rows of name, type
and value, written under the renderer's namespace (`arnold:global:` + the name)
unless the name has one of its own (`karma:object:geosamples` stays as typed).

**Custom:** type the renderer's name (`rray`). Its settings are rows you make,
each written under that name: `spp` (int 64) is `int rray:spp = 64`, `bounce` is
`rray:bounce`. Until it has a name nothing is written; the rows stay as you type
or change it.

**AOVs tab:** the renderer's standard AOVs as checkboxes, by group (Beauty,
Light paths, Surface, Geometry, IDs); the beauty (`C` for Karma, `RGBA` for
Arnold) is ticked to start with. **All**, **None** and **Defaults** tick them
together. Each ticked AOV becomes a RenderVar beside the product
(`/Render/Products/Vars/<name>`) with its `sourceName`, `sourceType`, `dataType`
and its name in the image (`driver:parameters:aov:name`, which husk needs on
every RenderVar), added to the product's `orderedVars`. When there is no product
yet, `Products/beauty` is made beside the RenderSettings prim. A Custom renderer
has no standard AOVs: add them with Render Variables.

### Renderer definitions

A renderer is a JSON file: its name, label and namespace, its settings and its
standard AOVs (the plan: [renderer-definitions.md](renderer-definitions.md)).

- **Built in:** Karma's and Arnold's are compiled into Imago
  (`assets/renderers/karma.json`, `arnold.json`): they match its version and
  cannot go missing.
- **The project's, over them:** the `.json` files of `$JOB/renderers`
  ([File > Variables](interface.md#variables) or the environment), read again
  when `$JOB` changes: a show pins its own definitions, and the farm renders
  with them.
- **Yours, over those:** your **renderers folder**, `$XMS_RENDERERS`, else
  `renderers` in Imago's config folder (`~/.config/xms/renderers` on Linux,
  `~/Library/Application Support/xms/renderers` on macOS,
  `%APPDATA%\xms\renderers` on Windows), read at launch and on **Reload**. A
  file there replaces the built-in one of its name, or adds a renderer. One
  that does not read is listed in the panel, with why, and the built-in one is
  used.
- **The Log tab** of Render Settings holds all this: the renderer as found
  here, where its definition comes from, the checks against Arnold, and the
  buttons below. Under the renderer chosen are only its version and, in red, a
  line pointing to the Log when it has something to say.
- **Make editable copy** (Render Settings' Log tab, File > Settings >
  Renderers) writes the built-in definition into your folder, with
  `"from": "<Imago's version>"`, and opens the folder. The panel says where
  each definition comes from ("Built in (Imago 0.2.5)", "Yours: …"), and,
  when the built-in one has settings or AOVs yours lacks (a later Imago), which.
  **Revert to built-in** renames yours aside (`arnold.json.reverted-<seconds>`).
- **Settings kept by name:** one set on an operator that the definition no
  longer has (under its namespace) is still written, and listed as not in the
  definition.

**Whether a renderer is on this machine**, each way Imago runs it, is shown in
the renderer menu ("not found here"), the panel and File > Settings >
Renderers; no renderer is ever hidden, so settings can be made for another
machine:

- **In Imago** (the `live` block): the renderer loaded and started as it is to
  render, and its version ("Arnold 7.5.0.0 in Imago (212 options)"), or why
  not. Not found, Render runs the command line instead, and the Viewer's
  Arnold is greyed, saying why.
- **Where each is installed** (a definition's `folder`): the **Arnold Folder**
  and the **Houdini Folder** in File > Settings > Renderers. Each sets its
  variable inside Imago (`$ARNOLD_PATH`, `$HFS`) over the environment's, for
  finding the renderer and for command lines, which also find its `bin` on
  their `PATH` (husk, kick). Programs opened from the Finder or the Dock on
  macOS do not see variables set in a shell's profile, nor its `PATH`: set the
  folders there.
- **The definition checked against Arnold:** once Arnold is found, every
  parameter of its `options` node is read from it (name, type, default, an
  enum's values). The panel and Settings say which settings of the definition
  are not Arnold options (so Arnold ignores them), and which defaults are not
  Arnold's own (most on purpose, as Houdini's Arnold ROP has them: Camera (AA)
  3, where Arnold's is 1). The options are kept in Imago's config folder with
  Arnold's version (`arnold_options.txt`): when a later Arnold is found, the
  options it added, dropped or changed are listed, to bring the definition up
  to date. The settings listed are the definition's: the Arnold ROP's
  Sampling, Ray Depth, Subdivision, Textures, System and Ignore, not all of
  Arnold's options.
- **As a command line** (`detect`): any of `env` (variables set), `programs`
  (on the `PATH`) and `files` (variables put in). Not found is only a warning:
  the command line still runs.

```json
"folder": { "label": "Houdini Folder", "variable": "HFS" },
"detect": { "env": ["HFS"], "programs": ["husk"], "files": ["$HFS/bin/husk"] }
```

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

Types: `bool`, `int`, `float`, `double`, `string`, `token`, `asset`, `float2`,
`float3`, `float4`, `color3f`. A setting with `"unconfirmed": true` says so in
its tooltip: its name follows the renderer's documentation as far as Imago
knows, but was not checked against the renderer.

An AOV can also give its RenderVar's `format` (`driver:parameters:aov:format`:
`color4h`, `float3`), `multiSampled`, and `extras`; `var_defaults` lists
attributes every RenderVar of the renderer carries. Karma's carry `clearValue`
and `karma:filter` (`["ubox",{}]`), as Houdini writes them.

**Karma's AOVs** follow what Houdini 20.5's Karma Render Settings writes: the
beauty `C` is the light path expression `C.*[LO]` (color4f, written as
`color4h`), the light path AOVs `color3h`, P `ray:hitP` and P (Camera Space)
`ray:hitP_camera` as `float3`, UV the `st` primvar. Those not seen in a Houdini
file are marked unconfirmed in their tooltip.

**Karma:** the settings' names, types and defaults are those Houdini 20.5's
Karma Render Settings writes: `karma:global:` for the render's
(`samplesperpixel`, `pathtracedsamples`, `convergence_mode`, `colorlimit`,
`bucketsize`…), `karma:object:` for the defaults every object takes (the ray
limits `diffuselimit`, `reflectlimit`… as floats, `varianceaa_*`,
`xformsamples`, `geosamples`). `karma --all-properties` lists every setting
Karma has, for adding more.

**Arnold:** arnold-usd reads Arnold's options from `arnold:global:` attributes
on the RenderSettings prim, each with the option's own name (`AA_samples`,
`GI_diffuse_depth`, `enable_adaptive_sampling`).

## Render Variables

RenderVars of a product, set or overridden row by row.

- **Product:** a path, or picked from the products coming in. Empty: the
  RenderSettings prim's first product, else the first coming in, else
  `/Render/Products/beauty` (made, and added to the scene's RenderSettings
  prim).
- **Replace:** on, the product holds these RenderVars alone, in this order. Off,
  they are added to its own; a row named as one it has replaces it.
- **Rows:** one RenderVar each, at `<product's parent>/Vars/<name>`, its name
  written as `driver:parameters:aov:name` too (an extra of that name overrides
  it):
  - **On**: off, the RenderVar is not written and is taken off the product
  - **Name**: the prim, and the AOV's name in the image
  - **Source Name**: an AOV name for raw, a light path expression for lpe
    (`C<RD>.*L`), a primvar's name. Empty: the name
  - **Source Type**: raw, lpe, primvar, intrinsic
  - **Data Type**: color4f, color3f, float, normal3f, point3f, vector3f…
  - **Extras**: what the renderer reads on the RenderVar beyond those, picked or
    typed: `driver:parameters:aov:format` (Karma's channel format: half,
    float…), `driver:parameters:aov:name`, `arnold:filter`, `arnold:width`
- **+ From a renderer** fills a row from any renderer's standard AOVs.

## Render Command

In the **System Commands** category of the add menu, drawn as a 90s terminal: a
black node with green text.

Runs a command line on the system: husk on the layer the Write USD above it
writes, say. Put it after that Write USD:

```
… ─► Render Settings ─► Write USD ─► Render Command
```

It runs **only when Run is pressed**, never while the graph cooks; on cook it
hands on what comes in.

- **Scene:** what `{usd}` is as it comes down the wires: the file of the nearest
  Write USD above it (a Set Variable below that can give another). **Write
  First** (on) writes that layer before running, so the render sees the scene as
  it is now; if it cannot be written, nothing runs and the panel says why.
  **Make Output Folders** (on) makes the folders the scene's RenderProducts
  write into before running (`render/` for `render/beauty.$F4.exr`, next to the
  layer as husk takes it): renderers do not make them.
- **Command:** the command line, with **Presets** to start from: husk with Karma
  CPU, Karma XPU, Arnold (arnold-usd's `HdArnoldRendererPlugin`), one husk per
  frame, Arnold's **kick** (`kick -i "{usd}" -frame $F -dw -dp -v 2`, one frame
  a run: it reads the layer's RenderSettings, camera and products through
  arnold-usd; add `-c` and `-o` to choose them), and an `echo` to try the
  variables. A preset of one frame (`$F`, not `$FSTART`) turns Run Per Frame on,
  one of a range turns it off. The line under it shows it with its tokens put
  in. husk renders the RenderSettings prim the layer names, so the presets do
  not give one (add `-s /Render/Settings` to choose).
- **Variables:** `{usd}`, the layer, and whatever else comes down the wires
  (Render Settings' `{delegate}` and `{renderer}`, Render Commons' `{settings}`
  and `{camera}`, a Set Variable's `{shot}`: see
  [Variables](interface.md#variables)); the built-ins with the run's values:
  `$F` the frame being rendered, `$F4` in four digits (`$F2` to `$F9` likewise),
  `$FSTART`, `$FEND` and `$N` the run's frames; and the other built-ins
  (`$OPNAME`, `$GNAME`, `$GDIR`, `$JOB`…, see
  [Variables](interface.md#variables)), then the environment. `${NAME}` sets a
  name apart, `$$` is a `$`, and a name that is none of these is left for the
  shell. Lines written before braces (`$USD`, `$NAME`) still work. Output Image
  in Render Settings has Imago's variables put in but not the frame's, which the
  renderer puts in per frame. The Folder and the Environment values take
  variables too (`{usd}` among them). Put quotes around `"{usd}"` for paths with
  spaces.
- **Frames:** the scene range, or a custom one. **Run Per Frame** runs the line
  once per frame, `$F` that frame, one after the other; off, it runs once for
  the range (husk renders ranges itself).
- **Environment:** the folder it runs in (empty: the layer's folder), and
  variables on top of Imago's (`HOUDINI_PATH`, `ARNOLD_PLUGIN_PATH`, licences).
  The program has to be on the `PATH` Imago was started with, or named by its
  full path.
- **Run:** Run, Stop and Clear, above its tabs; what it is doing, how long it
  has taken, and its progress, read from the percentages it prints (husk's
  `-V 2a` prints them).
- **Console tab:** what it prints, as a terminal shows it: light text on black,
  the command lines run in green, lines with errors or warnings in amber; the
  last 400 lines, Copy takes them. Pressing Run opens it; **Parameters** is the
  other tab. A Console pane for every run can follow.

It runs through the system's shell (`sh -c`; `cmd /C` on Windows), in the
background: Imago stays in use. Stop, or closing Imago, ends the command and
everything it started.

The layer holds the frames its Write USD writes (the scene's range by default),
what changes over them as time samples, so each frame of a range renders as it
is shown.

## Image Viewer

A pane of its own (a tab behind the Viewer in the default layout; Window menu
otherwise) for renders and images, on the `xms-image` crate.

**Rendering from it.** No Write USD or Render Command is needed: the Image
Viewer renders the scene the view flag shows.

- **RenderSettings** menu: the scene's RenderSettings prims, the one it names
  first (marked "the scene's").
- **Camera** menu: the RenderSettings prim's own camera, or any camera of the
  scene.
- **Render** writes the scene as a layer in a folder of Imago's own under the
  system's temporary folder (deleted when Imago closes, and, after a crash, when
  it next starts) and runs its command line on the current frame of it there, in
  the background. **Stop** ends it. The progress shows beside the buttons (from
  the percentages it prints), **Log** shows what it prints.
- **⚙** opens a small floating window of its own (a menu closed when its preset
  list opened). **Command:** the line it runs, to change, with presets: **husk**
  (`husk -R {delegate} -s "{settings}" -c "{camera}" -f $F -n 1 -V 2a --snapshot 2 --snapshot-suffix "" -o "{image}" "{usd}"`:
  it saves the image every two seconds while it renders, onto the image itself,
  and each save comes into the view, then the finished frame. Without
  `--snapshot-suffix ""` husk writes its snapshots beside it as
  `render.<frame>_part.exr`; the view reads those too, whichever file was
  written last), **kick**
  (`kick -i "{usd}" -frame $F -c "{camera}" -o "{image}" -dw -dp -v 2`, Arnold
  reading the layer through arnold-usd: the image comes in when the frame is
  done) and **Custom** (any other line: your renderer, a wrapper script). Under
  it, the line as it will run now. It is kept in Imago's config folder
  (`image_viewer_command.txt`); a husk path given before it existed starts the
  husk line. Its own variables, in braces, and the built-ins (see
  [Variables](interface.md#variables)); lines written before braces (`$USD`…)
  still work:

| Variable | Is |
|---|---|
| `{usd}` | The scene, written as `scene.usda` in the render folder |
| `{image}` | The image to write: `render.<frame>.exr` in the render folder. The view reads it |
| `$F`, `$F4`… | The frame, padded to that many digits |
| `{settings}` | The RenderSettings prim chosen |
| `{camera}` | The camera chosen, else the settings' own. With none known, its option (`-c "{camera}"`) is left out, so the renderer takes the settings' |
| `{delegate}` | husk's `-R`: `HdArnoldRendererPlugin` for Arnold settings; for Karma nothing, and `-R` is left out (husk's own, Karma) |
| `$$` | A `$`; a name that is none of these is the shell's (`$1`) |

- **⚙**, also: a file to open (any EXR, PNG, JPEG, TIFF, TGA, HDR). A program
  started from the macOS Dock does not see the shell's PATH: give the renderer's
  full path in the line
  (`…/Houdini.framework/Versions/Current/Resources/bin/husk`,
  `…/Arnold/bin/kick`).

**Looking at it.**

- **Layer**: the image's layers (AOVs), the beauty first. Channels are grouped
  by what is before their last dot (`combineddiffuse.r`, `.g`, `.b`); an EXR's
  named parts are layers too.
- **RGB** (a menu: RGB, R, G, B, A, L): the colour, one channel, alpha, or
  luminance. A layer of one channel is grey.
- **E** (exposure, in stops), **G** (gamma), **Reset**; **View** and **Display**,
  through OpenColorIO (see [Colour](colour.md)). The image is taken as in the
  rendering space (ACEScg); another space is chosen in ⚙.
- The wheel zooms about the pointer, a drag pans, a double-click, F or **Fit**
  fits the frame, and H or **1:1** shows it at 100%: one image pixel per screen
  pixel (on a Retina screen too), centred. The zoom at the top right is that
  ratio. The frame (display window) is outlined when the pixels go
  beyond it (overscan). The bottom corner shows the pixel under the pointer and
  every channel of the layer there, as floats.

**Snapshots**, the bar under the image, as Houdini's render view: **📷 Snap**
keeps the active image (the render coming in, or the file opened) as *Snap 1*,
*Snap 2*…; the menu, the slider and − and + go back and forth between them,
oldest first, and the **Active Render**, always last, so the newest snapshot is
beside it; the name field renames the one shown; **✕** lets it go. Rendering
again shows the active render; a snapshot keeps its layers, so compare any AOV.
They are kept while the program runs, with how much memory they take.

The snapshots are the program's, not the pane's: the Viewer has the same bar,
behind the **📷** at its bottom left (hidden to start with). There Snap keeps
what Arnold has rendered in the Viewer, every AOV, and choosing a snapshot
shows it in place of the scene, fitted, its aspect kept; **Viewer**, last,
shows the scene again. Each pane shows the one it chose, so snap in the
Viewer, then compare in the Image Viewer.

With a renderer that can (Arnold, for RenderSettings with Arnold settings),
Render renders in Imago itself, its image coming into the view bucket by
bucket; Stop stops it. Another renderer (Karma) runs the command line.

- **IPR**, beside Render, on by default: in passes, from few samples up, and
  each change to the graph goes to the renderer while it renders. Off: once,
  each bucket finished at the samples the Render Settings give, the scene as
  it was when Render was pressed, the buckets being rendered outlined in
  orange; Render is free again when it is done.

- Arnold is loaded from its SDK: the **Arnold Folder** in File > Settings >
  Renderers, else `ARNOLD_PATH` from [File > Variables](interface.md#variables)
  or the environment (the renderer definition's `live.sdk`). Arnold 7 or
  later; without a licence its images are watermarked. Imago looks for it at
  launch (see Renderer definitions), so Render starts at once.
- The scene is written as a layer in the render folder and Arnold loads it
  with its own USD reader. With IPR, each change to the graph after that goes to it
  while it renders, by prim path: a light's values, a MaterialX shader's
  inputs, a move, hiding a prim, moved points, the Render Settings' Arnold
  settings and resolution. Anything else (a prim added or removed, topology,
  a material's network, a camera's lens, another frame) writes the layer
  again and loads it.
- Imago's own display driver in Arnold takes each bucket as it is done, every
  AOV of it, into memory, beside the products' files (still written as they
  say): the Layer menu has them all, as an EXR's. Only the part of the layer
  shown that changed goes to the screen. The first pass can be
  long in coming while Arnold makes `.tx` files of new textures, once (they
  stay beside the textures).
- Each AOV's format in the file, half or float, is its RenderVar's
  `driver:parameters:aov:format`: Arnold's colour AOVs are half to start with
  (`color4h`, `color3h`), its data AOVs (P, N, Z, motion) float, as Karma's.
- It renders the layer's own RenderSettings and their camera. Arnold stays
  loaded after Stop, until Imago closes, so rendering again is quick.

- **In the Viewer:** the menu at the Viewer's top right, Viewport or
  Arnold. Arnold renders the scene there with IPR, through the Viewer's
  camera (its own, or the scene's camera looked through) at the pane's size,
  each change and each move of the view sent as you work; the wireframe and
  handles you work with are drawn over it. One Arnold for both panes: Render
  in the Image Viewer takes it back to the layer's camera, and Stop stops
  either.

### Headless: rendering without the window

```
xms --headless shot.xms --render 1-100
xms --headless shot.xms --render 1-100x2
xms --headless shot.xms            (the scene's range)
```

Imago opens the graph and cooks it frame by frame as in the window, ICE
deformers and every operator included, and Arnold renders each frame in
Imago's process at the Render Settings' samples. Arnold loads the scene once;
each frame after is sent to it as live edits, so nothing is cached to disk
between frames. The images are the Render Settings' products, written by
Arnold: relative ones from the scene's folder. Each frame's files are printed
as it is done; a frame not written, a scene with no product, or a render that
fails ends Imago with code 1. Arnold is found as in the window (the Arnold
Folder, else `$ARNOLD_PATH`), and the definitions of `$JOB/renderers` are read.

Later: rray rendering straight into it, frame sequences on the timeline, a wipe
between two images.

### Images from the graph: Load Image

**Load Image** (File) reads an image, or a sequence, into the graph: every layer
and channel of an EXR (AOVs grouped by name), and PNG, JPEG, TIFF, TGA and HDR
as RGBA. Its output is an **Image**: its pixels as stored, with the colour space
they are in.

- **When it is viewed** (its view flag), the Image Viewer shows its image, as the
  Viewer shows a viewed scene: the same layers, channels, exposure and pixel
  values as a render. A render, or a file opened from ⚙, takes over until the
  graph or the frame changes.
- **Sequences:** `$F4` in the file name is the frame in four digits
  (`plate.$F4.exr` at frame 12 reads `plate.0012.exr`); `$F`, `$F2` to `$F9`
  likewise, and `####` too. **Picking one frame of a sequence** in the file
  browser takes the whole sequence: `plate.0012.exr` becomes `plate.$F4.exr`
  when other frames are beside it (a numbered file on its own stays as it is).
  Variables are put in (`$GDIR/plates/plate.$F4.exr`), and the frame follows
  the timeline.
- **Its frames:** its Parameters show the file of the frame shown, its size,
  and the sequence's frames on disk ("frames 1001 to 1100"), with **Use as
  Scene Range** to set the timeline to them, as Load USD does for an animated
  stage. Selected, its frames show on the timeline as a clip's do.
- **Colour space, as in Nuke:** the space the file's pixels are in, chosen from
  the colour config. The Image Viewer turns them from that space through its
  view in one go, on the GPU as Gaffer and Houdini's COPs do: the image is
  uploaded once, as floats, and the transform is OpenColorIO's shader, so
  changing the space (here, or the Image Viewer's for a render), the view,
  the exposure or the channel shown is immediate at any size of image. An image
  too big for the GPU's largest texture is shown through the CPU instead.
- **Raw:** the Image Viewer always applies its view. An image (or a render) set
  to **Raw** is taken as in the rendering space and shown through the view
  chosen, so an 8-bit file read Raw through an sRGB view gets the curve twice;
  to see values exactly as stored, choose the **Raw view** as well. **Automatic** takes an EXR or HDR as in the rendering space,
  anything else as sRGB (the config's sRGB texture space). For a picture made
  for a screen (a screenshot, a JPEG of a render) choose the **display** space
  it was made for (`sRGB - Display`): that undoes the view transform, so through
  the same view it looks as it did. Only colour is converted (`R`, `G`, `B`),
  never an EXR's data layers.
- **Quick:** images read are kept (up to about a gigabyte), so scrubbing back over
  frames already seen does not read them again; a file changed on disk is read
  again.

### Image operators

Under **Image** in the add menu. Each takes an image and gives one, viewed in the
Image Viewer like Load Image. They work in the **rendering space (ACEScg)**,
**alpha premultiplied**, as Nuke and Gaffer do: an image coming in another space
(an 8-bit file read as sRGB) is brought into it first, once, and taken as having
straight alpha, so it is premultiplied then. Their pixels and frame are kept
apart, so images of different sizes and places line up by their frames.

| Operator | What it does |
|---|---|
| **Image Merge** | B (its first input) with each A laid over it in turn (A, A2…, one input kept free): Over, Under, Plus, Minus, Multiply, Screen, Max, Min, Difference, In, Out, Atop, Mask, Stencil; a Mix; the result's pixels every input's, B's or A's. The beauty is merged, other channels are B's |
| **Colour Correct** | Exposure, contrast about a pivot (0.18), saturation, gamma, gain and offset (per channel), clamps, in that order; the beauty, and other layers by name |
| **Shuffle** | Red, green, blue and alpha each from any channel (`R`, `diffuse.r`), from the second input (`2:R`), or a number; into the beauty or a layer of their own (`Into layer`) |
| **Delete Channels** | Channels deleted, or only some kept, by name (`depth.* P.?`) |
| **Constant** | A flat colour of a size |
| **Crop** | The pixels inside a box of the frame; optionally the frame too |
| **Reformat** | A new size, the picture fitted, filled or stretched; Box (good for shrinking), Bilinear or Cubic |
| **Transform 2D** | Moved, turned and scaled about a pivot, in frame pixels |
| **Blur** | Gaussian or box, across and down |
| **Premultiply, Unpremultiply** | The colour multiplied or divided by alpha |
| **Write Image** | Its image written when its **Write** is pressed: EXR with every channel as floats, or PNG, JPEG, TIFF, TGA as 8-bit with straight alpha; in a colour space (Automatic: EXR in the rendering space, others sRGB); a frame, the scene's range or its own (`$F4` in the name). Its image passes through |

They work on the CPU, rows on every core; the Image Viewer shows their result on
the GPU.

## In the scene

The render prims are primitives with no geometry, as cameras and materials are:
the Scene Explorer lists them (⚙ RenderSettings, 🖼 RenderProduct, 📑 RenderVar)
under their Scopes, and the Primitive Inspector's Properties tab lists their
attributes and relationships, with the render operator they came from. The
Viewer does not draw them.

Write USD writes them as `def RenderSettings`, `def RenderProduct` and
`def RenderVar` under `def Scope` parents, and names the RenderSettings prim in
the layer's header (`renderSettingsPrimPath`). One that came from a loaded file
is written as an `over` with what changed; its relationships are appended
(`append rel orderedVars = [...]`), since Imago does not read the file's own
targets yet.

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
        int karma:global:pathtracedsamples = 128
        int karma:global:samplesperpixel = 9
        uniform float pixelAspectRatio = 1
        uniform int2 resolution = (1920, 1080)
        rel camera = </camera>
        rel products = </Render/Products/beauty>
    }
}
```

Render the layer with husk and Karma, or with Arnold through arnold-usd (husk or
kick): the renderer reads the RenderSettings prim the layer names.

## Not done yet

- Rendering inside Imago (the Viewer's Render mode, rray as its render
  delegate), and the image streamed into the Viewer while husk renders. The
  plan, starting with Arnold driven live in Imago's process, is
  `docs/arnold-native.md`; its first step, a prototype outside Imago, is
  `prototypes/arnold`, its second the renderer interface and the Arnold
  backend, `crates/xms-render`
- Several Render Commands queued, or sent to a farm
- A file's products and RenderVars read as relationships
- A camera or resolution per product
- Settings that change over time
