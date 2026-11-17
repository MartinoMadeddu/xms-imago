# Colour

Imago manages colour with OpenColorIO, through the pure-Rust `ocio` crate
(`crates/xms-colour`). One config serves the whole program.

## The config

- **`$OCIO`**, when it names a config that loads: a `.ocio` file, an `.ocioz`
  archive, or one of OpenColorIO's built-in configs by its URI
  (`ocio://studio-config-v4.0.0_aces-v2.0_ocio-v2.5`).
- **Imago's own** otherwise, built into the program, so nothing has to be set
  up: `crates/xms-colour/imago.ocio`. It is a part of the ACES CG config
  (cg-config v2.2.0, ACES 1.3, OCIO 2.4) and reads no files:
  - **ACEScg** the working space (`scene_linear`, `rendering`), as Houdini's
    and Karma's ACES pipeline has it;
  - displays **sRGB - Display** (the default: most monitors), **Rec.1886
    Rec.709 - Display** (gamma 2.4, broadcast reference monitors) and
    **Display P3 - Display**;
  - views **ACES 1.0 - SDR Video** (the default), **Un-tone-mapped** and
    **Raw**;
  - colour spaces ACES2065-1, ACEScg, ACEScct, Linear Rec.709 (sRGB), Linear
    P3-D65, sRGB Encoded Rec.709 (sRGB) and Raw.

When `$OCIO` names a config that does not load, Imago's own is used, and an
**OCIO** mark beside the colour menus (File > Settings > Viewer, the Image
Viewer's top bar) says why. The config is read once, when
Imago starts.

## The scene's colour space

The scene's colours are in USD's default colour space, linear Rec.709:
materials' colours, `displayColor`, lights, and textures once read (sRGB
textures are decoded to linear), as USD files author them. They are kept as
they are and turned into the working space (ACEScg) on the way to the display,
one matrix in the view's transform: taking them as ACEScg would saturate every
colour. Imago finds linear Rec.709 in the config by its name or an alias
(`lin_rec709_scene`, `Linear Rec.709 (sRGB)`, `lin_rec709`…); a config without
one has its `scene_linear` role taken instead, and the OCIO mark says so.

## Viewer

The Viewer draws the scene in linear light, in half floats, so light brighter
than 1 is kept. It shows the **View** (ACES 1.0 - SDR Video, Un-tone-mapped,
Raw) and **Display** (sRGB, Rec.1886 Rec.709, Display P3) the Image Viewer's
menus choose: one colour for both, **As Image Viewer** in File > Settings >
Viewer, on to start with. Off, the Viewer has its own View and Display there,
starting from what it showed; the page also says which config is used, so the
Viewer's top bar stays clear.
Changing the display keeps the view when the display has it. The transform runs on the GPU: OpenColorIO's own shader
for the view, written as WGSL, LUTs and all. Its overlays (the grid, the
handles, selection) go through it too. Each pane's choice is remembered.
Export Movie writes the Viewer's frames through the same View and Display (see
[Interface](interface.md#export-movie)).

## Image Viewer

View and Display menus in its top bar, as Houdini's and Nuke's image viewers
have them: ACES 1.0 - SDR Video on Rec.1886 Rec.709 - Display until others
are chosen (the config's own default when it has no Rec.1886 display). An
image goes through the transform on the
CPU from its colour space: the config's **rendering space** (ACEScg), what
renders are in, or another of the config's spaces chosen in the ⚙ settings
(Image colour space), for an image that is not a render. Exposure is applied
before the transform, gamma after; the sRGB switch is not needed and is not
shown. Alpha is shown as it is.

## Renders

Arnold renders with Imago's colour management, so a render is in the space the
Image Viewer takes it to be: after the scene loads, Imago gives Arnold an OCIO
colour manager (`color_manager_ocio`) with

- **config**: the one Imago uses, `$OCIO`'s file, or Imago's own (written
  beside the render's scene, `colour/imago.ocio`);
- **rendering space** (`color_space_linear`): the config's `rendering` role,
  else `scene_linear` (ACEScg in Imago's);
- **8-bit textures' space** (`color_space_narrow`): sRGB texture (`srgb_texture`
  and its usual names), else the `texture_paint` role: what an 8-bit texture
  with no colour space of its own is taken to be.

A texture's own Colour Space (MaterialX, USD's `colorSpace`) is converted from
into the rendering space. A scene that gives Arnold its own colour manager (in
its Render Settings) keeps it, and the render says so. Live renders, renders
from Imago and headless renders all do the same.

Colours typed as values (a base colour) are taken as they are, in the
rendering space, as Arnold does; the Viewer takes them as linear Rec.709,
USD's default, and turns them into ACEScg.

## Not done yet

- The Viewer showing a texture in the colour space it is given (MaterialX's
  Colour Space, `colorSpace`, `sourceColorSpace`: kept and written for the
  renderer, see [MaterialX](materialx.md)): it takes 8-bit files as sRGB, float
  files as linear
- Colours typed as values converted for the renderer as the Viewer converts
  them (USD's `renderingColorSpace`, the colour spaces of values)
- Looks, and the Viewer's own exposure and gamma
- Choosing the config inside Imago (it is `$OCIO`, or Imago's)
- Overlays drawn after the transform, so they keep their colours under a
  tone-mapping view
