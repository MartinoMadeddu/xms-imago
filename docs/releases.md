# Builds and releases

Two kinds of download, both on the [Releases
page](https://github.com/MartinoMadeddu/xms-imago/releases), both built by
GitHub Actions for Linux, macOS and Windows:

- **Builds:** every push to `main` that changes code is built and published as
  a pre-release, "Build N". A push never changes the version: build 154 and
  build 155 are both builds of the version `Cargo.toml` says, with what was
  pushed in between. Changes to documentation alone do not trigger a build.
- **Releases:** when a set of work is done, a version is released by hand, as
  below: "Imago 0.3.0", marked latest. The website's download buttons fetch
  the latest release, not the newest build.

Imago shows which it is beside its name, top left: `0.2.0 · build 155
(a1b2c3d)` for a build from GitHub (click it to copy it, for a bug report),
`0.2.0 (local)` when built on your own machine. A saved graph records it too
(`saved_with`).

## Versions

One version for the whole program: the root `Cargo.toml`'s
`[workspace.package] version`, which every crate takes (`version.workspace =
true`). Before 1.0 it reads `0.MINOR.PATCH`:

- **MINOR** (0.2 to 0.3): a milestone, or a change older Imagos cannot read.
- **PATCH** (0.2.0 to 0.2.1): fixes only.

A file's own layout has its own number, apart from the program's (a graph's
`version`, a compound's `format_version`), changed only when the layout does.

## Making a release

1. In `CHANGELOG.md`, move what is under **Unreleased** under a new heading,
   `## [0.3.0] - <date>`.
2. Set `version = "0.3.0"` in the root `Cargo.toml`'s `[workspace.package]`,
   and run `cargo build` once so `Cargo.lock` follows.
3. Commit, tag and push:

       git commit -am "Imago 0.3.0"
       git tag -a v0.3.0 -m "Imago 0.3.0"
       git push && git push origin v0.3.0

The tag builds the release, named "Imago 0.3.0", with the changelog's section
as its notes. A tag that does not match the version in `Cargo.toml` fails the
build, so nothing is published under a wrong number. `git config --global
push.followTags true` makes a plain `git push` send the tag too.

| File | Platform |
|---|---|
| `xms-0.2.0-linux-x86_64.tar.gz` | Linux, 64-bit Intel or AMD |
| `xms-0.2.0-windows-x86_64.zip` | Windows, 64-bit |
| `xms-0.2.0-macos-arm64.tar.gz` | macOS, Apple silicon |
| `xms-0.2.0-macos-x86_64.tar.gz` | macOS, Intel |

Every name carries the version, and a build of `main` its number too:
`xms-0.2.0-build155-linux-x86_64.tar.gz`. So does the folder inside and the
program in it (`xms-0.2.0`, `xms-0.2.0-build155`, `.exe` on Windows), so
several versions and builds can be unpacked side by side. Builds are listed on
the Releases page with the releases, newest first, marked Pre-release; the
page's Latest is always a release.

Each archive holds the program (`xms-0.2.0`, or `xms-0.2.0.exe`; Cargo builds it as
`xms-imago`), the README, this documentation, the `examples` folder and the
`compounds` folder (the compound definitions Imago ships, from
`assets/compounds`). Keep both next to the program: the templates that load
files look for `examples` there, and the library finds the shipped compounds
there (see [compounds](compounds.md)).

- Linux: built against glibc 2.28, so it runs on Rocky Linux 8, RHEL 8, Ubuntu
  20.04 and anything newer. It needs the X11 or Wayland libraries (libX11,
  libXcursor, libXi, libxkbcommon) and a Vulkan or OpenGL driver, which desktop
  installs have; they are loaded when the program starts.
- macOS: the program is not signed. On first run, right-click it and choose
  Open, or run `xattr -d com.apple.quarantine xms-0.2.0` (the program's name
  in the folder).
- Windows: SmartScreen may warn about an unsigned program.

The workflow is `.github/workflows/release.yml`. It can also be started by hand
from the Actions tab. The unit tests run on the Linux build. The Linux binary is
linked by `cargo zigbuild` against glibc 2.28, not the runner's own, and the
build fails if it asks for anything newer.

To build the same binary on your own machine:

    pip install --user ziglang
    cargo install cargo-zigbuild
    cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.28

The program is then in `target/x86_64-unknown-linux-gnu/release/xms-imago`.

## Building from source

    cargo run --release

On Ubuntu or Debian, first:

    sudo apt install build-essential pkg-config libasound2-dev libudev-dev libx11-dev libxkbcommon-x11-0

## Website

The page at https://martinomadeddu.github.io/xms-imago/ is the `site/` folder of
the repository, published by `.github/workflows/pages.yml` on every push to
`main` that touches the site, the screenshots in `docs/` or the artwork in
`assets/`.

The downloads are in four places: a bar that stays at the top of the window,
whose button fetches the build for the visitor's machine, a band of four large
buttons under the cover, the cover disk, and a second band at the end. Its
download buttons point at `releases/latest/download/<file>`, so they always
fetch the newest build without the page being touched. The page also names the
newest build, from GitHub's public record of it.

The page is laid out as a 3D magazine of the 1990s: a cover, a contents page
with the downloads on a cover disk, then a double page in its own colours for
each area of the program, a reference card with the keys, the file formats and
all 38 nodes, the build instructions as a type-in listing, and the known gaps.
Its fonts are in `site/fonts/` and are served with it.

The adverts down the margins are parodies of the period, for products that do
not exist: seventeen GIFs in `site/ads/`, six of them animated. They are not all
of one kind: 16-colour dithered ones, a four-colour one, an amber text screen, a
one-ink newspaper halftone, and 48 to 128 colour ones, one of them a proper
raytrace. Three carry a rendered "photograph" given the grain, the cast and the
crooked scan of a bad print. Three carry real photographs of Martino and Simon.
They are pictures only and cannot be clicked. The margins exist in windows 1640
pixels wide or more, and the adverts grow with them, up to 380 pixels wide; in
narrower windows twelve of them sit on a page of their own before the credits.

![The website](xms_site.png)

One-time setup on GitHub: Settings > Pages > Source: "GitHub Actions". With
"Deploy from a branch" selected instead, GitHub also publishes the README as a
plain page at the same address, and whichever of the two finished last is what
visitors get.

To look at the site before pushing:

    sh site/build.sh _site

then open `_site/index.html`.
