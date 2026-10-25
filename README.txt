# termdir

`td` is the fast terminal directory explorer for TRUEOS.

Run from this directory:

    cargo run

Backend selection
-----------------
The per-launch script accepts `backend terminal` or `backend ui4`.
CLI equivalents: `--backend terminal` and `--backend ui4`.
Invalid backend values fail before acquiring a terminal or opening a window.
Omitting the directive retains the terminal backend for existing launchers.

Both local Shell3 UI4 windows and SSH use the terminal backend and its
terminal lease. The old native `ui4` renderer has been retired; requesting it
returns an unsupported-backend error. Renderer selection does not identify
whether the active terminal is local or remote.

Edit action
-----------
Select one `.txt` file in a local UI4 Shell3 window to reveal `edit` at the top
of the Action list. It launches the built-in `edit` Blueprint in a separate
Shell3 window, leaving termdir open. The child receives a one-shot runscript:

    fs-scope trueosfs
    open trueosfs:discN/path/to/note.txt

The `open` directive treats the remainder of its line as a literal filename,
including spaces. Filenames containing line breaks cannot use this action.
The live `vFile:terminal-frontend` query distinguishes `ui4`, `remote`, and
`detached`; termdir refreshes it on terminal re-entry and before launching.
The action is hidden on SSH and for multiple selections, folders, and other
extensions. Remote editor handoff is not implemented by this action.

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

  termdir declares `fs-scope = "trueosfs"` in its Blueprint package metadata,
  so ordinary launches already receive this capability and default to `/`.
  An explicit browse path overrides that default. The launch-script directive
  above also grants the scope (including for older termdir packages).
  Without either grant, the standard v filesystem API maps paths to the app
  root plus `/common`. With the grant, termdir can enumerate every mounted
  TRUEOSFS root and shows those roots at the top of the Mount menu. `★` marks
  the primary root and `◇` marks a read-only root. Root selectors have the form
  `trueosfs:discN`.

  This grant gates mount enumeration and cross-disk selectors. It is not yet a
  general filesystem security boundary: legacy raw async-filesystem vmcalls can
  still address valid paths on the primary root, and TRUEOSFS RecordKey values
  are currently identity metadata rather than enforced encryption/access keys.

Controls
--------
- LMB: click a file to toggle it into a same-folder file group; folders and
  files from another folder start a new selection
- MMB drag: pan
- Arrow keys or W/A/S/D: pan
- Home: center
- Enter: enter selected folder
- Tab / Shift+Tab: cycle Mount, View, Action
- 0..9: run the local numbered entry in the active menu section
- Mount section: 0 refreshes; 1..9 open the listed TRUEOSFS roots
- View `zoom`: cycles 75%, 100%, 125%, 150%, and 200% terminal text
- Mouse hover: move the menu cursor and activate that section
- Drag a selected file group onto a folder: confirm moving the whole group
- Drag a selected file group onto a visible top-path crumb: confirm moving it
  into that folder
- Top path and bottom selection titles use centered `・・` rule bridges
- `sha ＃` remains a single-file SHA-256 action; it is unavailable for groups
- `show` appears only for inferred PNG/JPEG files, same-folder image groups,
  or folders containing an inferred PNG/JPEG. It launches one new resident
  img instance; groups open one fixed UI4 frame per selected file.
- `ply` appears for one inferred MP4 file and opens it through Shell2's
  `vid fs` player with looping enabled.
- Select a file or folder and choose `7z 🗜` to pack it; selecting a `.7z`
  archive extracts it beside the archive using a collision-free output name
- Choose `7z 🗜` with a file group to bundle every selected file into a
  collision-free `bundle.7z` in their shared folder
- Drag a selected file group into virtual columns 0, 1, or 2: confirm recycling
  the whole group
- Drag a selected file group into the menu: pin every member as a session Clip
  link (no filesystem move)
- Click and release a folder link: mount it
- Click and release a file link: mount its parent and select it when visible
- Drag a Clip link onto a folder or breadcrumb: confirm moving the linked item
- Drag a Clip link out and release without a folder target: unpin it
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
- Confirmed delete moves the item or selected file group to .explorer-trash/.
- The current graph is not rescanned or re-laid out.
- The deleted entry stays at the same position as `🪦 removed` until the next reload/mount/parent/depth refresh.
- Removed folder descendants are hidden immediately and are non-interactive.

Move behavior
-------------
- Confirmed moves rename the item or selected file group on disk without
  rescanning or re-laying out the current graph.
- The old entry stays at the same position as `moved` until the next reload/mount/parent/depth refresh.
- Moved folder descendants are hidden immediately and are non-interactive.
