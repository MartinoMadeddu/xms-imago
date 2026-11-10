# Renderer definitions (plan)

Agreed 7 November 2026, revised 8 November after native Arnold (see
[arnold-native.md](arnold-native.md)). What is built is folded into
[render.md](render.md), "Renderer definitions"; this file keeps the plan and
what is left of it.

## The aim

A renderer's settings and AOVs live in a JSON file, so they can be added to,
corrected or replaced after Imago is built, by editing a file and reloading,
with no rebuild. Imago tells you whether each renderer is installed on the
machine, each way it runs it, without ever hiding one.

## Decisions

### Where definitions come from

- **Built in (compiled with the program): the defaults.** They match the
  program's version, cannot go missing or be broken by an edit, and are what
  tests use (`assets/renderers/*.json`).
- **Yours, over the defaults.** Your renderers folder (`$XMS_RENDERERS`, else
  `renderers` in Imago's config folder) is read at launch and on **Reload**. A
  file there replaces the built-in definition of the same name, or adds a new
  renderer.
- **No definitions shipped as files beside the program.** Files there that
  Imago never loads are what people would edit, see nothing change, and lose
  at the next version. The download has a `renderers/README.md` saying where
  yours go and how to make one; **Make editable copy** writes the starting
  point.

### Making and keeping your own

- **Make editable copy** (the Render Settings panel, and File > Settings >
  Renderers) writes the renderer's built-in definition into your folder
  (`<folder>/arnold.json`), with `"from": "<Imago's version>"`, and opens the
  folder. From then on yours is loaded in its place.
- **Where a definition comes from is shown:** "Built in (Imago 0.2.5)" or
  "Yours: `…/renderers/arnold.json`".
- **Yours does not go stale silently.** A copy is the whole definition, so a
  later Imago's new settings are hidden by it: when the built-in definition
  has settings or AOVs yours lacks, the panel says how many and which, with
  the version yours is from. (Later, perhaps: a file that layers over the
  built-in setting by setting.)
- **Revert to built-in** renames yours aside (`arnold.json.reverted-<date>`),
  never deleting it.

### Detecting what is installed

Each way Imago runs a renderer is found on its own: Arnold can work through
husk and not in Imago, or the reverse.

- **In Imago** (a definition's `live` block): the renderer is loaded, as it
  will be to render, and asked its version: "Arnold 7.5.0.0 in Imago", or why
  not ("no libai.dylib in …/bin"). Where it is: the **Arnold folder** in File
  > Settings > Renderers, else the `live` block's `sdk` (`$ARNOLD_PATH`). The
  setting comes first because programs opened from the Finder or the Dock on
  macOS do not see variables set in a shell's profile.
- **As a command line** (a `detect` block): any one match counts as found.

  ```json
  "detect": { "env": ["HFS"], "programs": ["husk"], "files": ["$HFS/bin/husk"] }
  ```

  Paths may use variables (`$HOME`, `$JOB`…).
- **Never hide a renderer.** Every renderer is listed, with its settings, so
  render settings can be authored for another machine (a farm, a colleague),
  and a graph saved with them always opens with them.
- **Mark it instead**, in the renderer menu, the panel and Settings.
- **What it gates:**
  - In Imago: not found, Render runs the command line instead, and the
    Viewer's Arnold is greyed, saying why.
  - The command line: never disabled, only warned about (detection can be
    wrong: farm wrapper scripts, an unusual `PATH`).

### Shaders

A definition's `shaders` block makes the renderer's own shaders nodes of
MaterialX graphs (see [MaterialX](materialx.md#renderers-shaders)):

```json
"shaders": { "prefix": "ai_", "context": "arnold", "shipped": "arnold_shaders.mtlx",
             "sdk": "$ARNOLD_PATH/materialx/arnold/nodedefs.mtlx" }
```

- `prefix`: before each node's name, apart from MaterialX's (`ai_add` is not
  `add`).
- `context`: the USD render context and shader namespace they are written in
  (`arnold:standard_volume`, `outputs:arnold:volume`).
- `shipped`: the definitions always there, a MaterialX file beside the
  definition (Imago's own: `assets/renderers`).
- `sdk`: the renderer's own MaterialX definitions, read when Load Shader
  Definitions from SDK is on (File > Settings > Renderers, or
  `XMS_RENDERER_SHADERS=1`). Any renderer shipping a MaterialX file of its
  shaders works the same way.

### Reloading

- **Reload** (the panel, and Settings) reads your folder again and finds the
  renderers again, with no relaunch.
- **Settings already set in graphs are kept by name.** One no longer in the
  definition is still written to the RenderSettings prim (when it is under the
  renderer's namespace) and listed as "not in this renderer's definition",
  rather than dropped.

## Steps

Steps 1 to 4 are built (`src/core/renderers.rs`; `xms_render::arnold::installed`
for Arnold in Imago).

1. **Detection:** the `live` check (loading the renderer, its version) and the
   `detect` block; the Arnold folder setting; the result shown in the menu,
   the panel and Settings; Render and the Viewer gated by it.
2. **Where definitions come from:** each knows its source; Make editable copy
   (with `from`), what yours lacks, Revert to built-in, Reload; settings kept
   by name.
3. **Packaging:** the release workflow puts `renderers/README.md` in every
   download.
4. **Docs:** render.md's "Renderer definitions" brought up to date.

Tests with each step: detection with a variable set or not, a program on a
temporary `PATH`, a file path with a variable, a renderer that does not load; a
user definition replacing a built-in one and adding one; a broken file listed
with its reason, the built-in still used; Make editable copy and Revert; what
yours lacks; a setting kept by name across a reload that removes it.

## Then (built)

- **The definition checked against Arnold** (`check`): its settings Arnold
  does not have, its defaults that are not Arnold's, and, with each Arnold's
  options kept with its version, what a new version added, dropped or
  changed. The settings listed stay the definition's (the Arnold ROP's
  Sampling, Ray Depth, Textures…), not all of Arnold's options.
- **Folders as variables:** a definition's `folder` (`Houdini Folder`, `HFS`),
  set in Settings, is that variable inside Imago, and its `bin` is on the
  command lines' `PATH`.

- **A project layer:** `$JOB/renderers`, between the built-in definitions
  and yours, read again when `$JOB` changes: a show pins its own, and the farm
  renders with them. Yours replaces the project's of the same name, and the
  Log says so; Make editable copy of the project's copies its file; Revert is
  only for yours.

## Later

- **Make editable copy with Arnold's options:** writing the options read
  from your Arnold into your copy, to group and label. Karma's from
  `karma --all-properties`.

