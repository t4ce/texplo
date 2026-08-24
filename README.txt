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

Initial path
------------
- Native/CLI: `cargo run -- --browse /path/to/open`
- TRUEOS launch vFile:

      fs-scope trueosfs
      browse /
      depth 2

  `fs-scope trueosfs` is a host-side capability grant. Without it, the standard
  v filesystem API keeps mapping paths to the app root plus `/common`, and
  `browse /` means the app root. With it, Texplo can enumerate every mounted
  TRUEOSFS root and shows those roots at the top of the Mount menu. `★` marks
  the primary root and `◇` marks a read-only root. Root selectors have the form
  `trueosfs:discN`.

  This grant gates mount enumeration and cross-disk selectors. It is not yet a
  general filesystem security boundary: legacy raw async-filesystem vmcalls can
  still address valid paths on the primary root, and TRUEOSFS RecordKey values
  are currently identity metadata rather than enforced encryption/access keys.

Controls
--------
- LMB: select / drag one file or folder
- MMB drag: pan
- Arrow keys or W/A/S/D: pan
- Home: center
- Enter: enter selected folder
- Tab / Shift+Tab: cycle Mount, View, Action
- 0..9: run the local numbered entry in the active menu section
- Mount section: 0 refreshes; 1..9 open the listed TRUEOSFS roots
- View `zoom`: cycles 75%, 100%, 125%, 150%, and 200% terminal text
- Mouse hover: move the menu cursor and activate that section
- Drag onto a folder: confirm move
- Drag onto a visible top-path crumb: confirm move into that folder
- Top path and bottom selection titles use centered `・・` rule bridges
- Select a file and choose `sha ＃` for its cross-platform SHA-256 digest
- Select a file or folder and choose `7z 🗜` to pack it; selecting a `.7z`
  archive extracts it beside the archive using a collision-free output name
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

Move behavior
-------------
- Confirmed moves rename the item on disk without rescanning or re-laying out the current graph.
- The old entry stays at the same position as `moved` until the next reload/mount/parent/depth refresh.
- Moved folder descendants are hidden immediately and are non-interactive.
