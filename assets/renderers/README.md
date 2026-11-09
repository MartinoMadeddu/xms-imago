# Your renderer definitions

Imago is built with Karma's and Arnold's definitions (their settings and
AOVs). To change one, or add a renderer, put a JSON file in **your renderers
folder**, not here: files in this folder are not read.

- Your folder: `$XMS_RENDERERS`, else `renderers` in Imago's config folder:
  `~/.config/xms/renderers` (Linux), `~/Library/Application
  Support/xms/renderers` (macOS), `%APPDATA%\xms\renderers` (Windows).
- **Make editable copy**, in the Render Settings panel or File > Settings >
  Renderers, writes a built-in definition there to start from; **Reload**
  reads your folder again; **Revert to built-in** renames yours aside.
- A file of yours replaces the built-in one of its name, or adds a renderer.

The format: `docs/render.md`, "Renderer definitions".
