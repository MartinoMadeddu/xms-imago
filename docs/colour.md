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
  - displays **sRGB - Display** and **Display P3 - Display**;
  - views **Un-tone-mapped** (first: what the Viewer showed before colour
    management), **ACES 1.0 - SDR Video** and **Raw**;
  - colour spaces ACES2065-1, ACEScg, ACEScct, Linear Rec.709 (sRGB), Linear
    P3-D65, sRGB Encoded Rec.709 (sRGB) and Raw, with Linear Rec.709 (sRGB) as
    `scene_linear`.

When `$OCIO` names a config that does not load, Imago's own is used and the
colour menus say why. The config is read once, when Imago starts.

## The scene's colour space

The scene is in USD's default colour space, linear Rec.709: materials' colours,
`displayColor`, lights, and textures once read (sRGB textures are decoded to
linear). Imago finds that space in the config by its name or an alias
(`lin_rec709_scene`, `Linear Rec.709 (sRGB)`, `lin_rec709`…); a config without
one has its `scene_linear` role taken instead, and the colour menus say so.

## Viewer

The Viewer draws the scene in linear light, in half floats, so light brighter
than 1 is kept; the view chosen in its top bar's colour menu turns it into what
the display is sent. The menu lists each display's views, the config in use
and the scene's space. The transform runs on the GPU: OpenColorIO's own shader
for the view, written as WGSL, LUTs and all. Its overlays (the grid, the
handles, selection) go through it too. Each pane's choice is remembered.

## Image Viewer

An image goes through the same transforms on the CPU, from its own colour
space: **the scene's** by default, or any of the config's from the menu next to
the view's (a render from an ACES pipeline is in ACEScg). Exposure is applied
before the transform, gamma after; the sRGB switch is not needed and is not
shown. Alpha is shown as it is.

## Not done yet

- The colour space of a texture as USD names it (`sourceColorSpace`,
  `colorSpace` metadata): textures are sRGB or raw
- Looks, and the Viewer's own exposure and gamma
- Choosing the config inside Imago (it is `$OCIO`, or Imago's)
- Overlays drawn after the transform, so they keep their colours under a
  tone-mapping view
