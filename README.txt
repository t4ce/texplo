explorer_tui_v13
================

Run from this directory:

    cargo run

Dependency policy: std + crossterm only.

Diagnostic build marker (when diagnostics are enabled):
    v0.13 · tombstone delete

Diagnostics are hidden by default so the graph and menu use the full terminal.
Enable the six diagnostic rows only when needed:

    cargo run -- --diagnostics

Aliases: --diagnostic, --diag, -d
Environment form: EXPLORER_DIAGNOSTICS=1 cargo run
Use --no-diagnostics to override an enabled environment flag.

Controls
--------
- LMB: select / drag one file or folder
- MMB drag: pan
- Arrow keys or W/A/S/D: pan
- Home: center
- Enter: enter selected folder
- Tab / Shift+Tab: cycle Mount, View, Action
- 0..9: run the local numbered entry in the active menu section
- Mouse hover: move the menu cursor and activate that section
- Drag onto a folder: confirm move
- Drag into virtual columns 0, 1, or 2: confirm recycle move
- Drag into the menu: add a session link (no filesystem move)
- Click a folder link: mount it
- Click a file link: mount its parent and select it when visible
- Y/N or mouse: confirmation modal
- Enter/Esc: accept/cancel text-input modal

Rendering notes
---------------
- A retained terminal-cell framebuffer repaints only changed runs while panning.
- Braille connector geometry is cached in world space and only translated/clipped
  during pan and terminal resize.
- The edge cache is row-sorted, so pan only scans connector rows intersecting the
  current viewport.
- Bursts of drag/key-repeat events are coalesced before repainting.
- Resize events are debounced; resize repaint does not clear the whole screen.

Delete behavior
---------------
- Confirmed delete moves the item to .explorer-trash/.
- The current graph is not rescanned or re-laid out.
- The deleted entry stays at the same position as `🪦 removed` until the next reload/mount/parent/depth refresh.
- Removed folder descendants are hidden immediately and are non-interactive.
