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
than 1 is kept. Its **View** (ACES 1.0 - SDR Video, Un-tone-mapped, Raw) and
**Display** (sRGB, Rec.1886 Rec.709, Display P3) are in File > Settings >
Viewer, which also says which config is used, so its top bar stays clear.
Changing the display keeps the view when the display has it. The transform runs on the GPU: OpenColorIO's own shader
for the view, written as WGSL, LUTs and all. Its overlays (the grid, the
handles, selection) go through it too. Each pane's choice is remembered.

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

## Not done yet

- The colour space of a texture as USD names it (`sourceColorSpace`,
  `colorSpace` metadata): textures are sRGB or raw
- Looks, and the Viewer's own exposure and gamma
- Choosing the config inside Imago (it is `$OCIO`, or Imago's)
- Overlays drawn after the transform, so they keep their colours under a
  tone-mapping view
