<p align="center">
  <img src="assets/termdir-logo.png" alt="termdir logo" width="360">
</p>

termdir is a fast terminal directory explorer. Its short command is `td`.
This `otheros` branch carries the portable UI and filesystem behavior developed
in the TRUEOS application while using crossterm and the host filesystem.

The project is distributed under the BSD 2-Clause license. See `LICENSE`.

<a href="https://snapcraft.io/termdir">
  <img alt="termdir" src="https://snapcraft.io/termdir/badge.svg" />
</a>

Run from this directory:

    cargo run

Open a particular directory:

    cargo run -- --browse /path/to/open
    cargo run -- /path/to/open

Diagnostics are hidden by default. Enable the six diagnostic rows with
`--diagnostics`, `--diagnostic`, `--diag`, `-d`, or `TERMDIR_DIAGNOSTICS=1`.

Controls
--------
- LMB: select or drag a file/folder
- MMB drag: pan
- Arrow keys or W/A/S/D: pan
- Home: center
- Enter: enter the selected folder
- Tab / Shift+Tab: cycle Mount, View, Action, and Clip
- 0..9: run the local numbered item in the active menu section
- View zoom: cycle 75%, 100%, 125%, 150%, and 200%
- Drag onto a folder or breadcrumb: confirm move
- Drag into virtual columns 0, 1, or 2: confirm recycle move
- Drag into the menu: add a session Clip link
- Click and release a folder link: mount it
- Click and release a file link: mount its parent and select it
- Drag a Clip link onto a folder or breadcrumb: confirm moving the linked item
- Drag a Clip link out and release without a folder target: unpin it
- Y/N or mouse: confirmation modal
- Enter/Esc: accept/cancel text input
- Ctrl+Q or Esc: exit

Details carried over from TRUEOS
--------------------------------
- Compact 17-cell Mount/View/Action/Clip menu with selection statistics
- Tree/radial layout, line-style, depth, spacing, zoom, and minimap controls
- Clickable middle-dot breadcrumbs and drag-to-breadcrumb moves
- New file/folder, rename, recycle, SHA-256, and retained move/delete markers
- Directory-first case-insensitive sorting
- Buffered retained-cell rendering, Unicode cell widths, resize debounce, and
  coalesced input bursts
- Folder subtrees retain a visual gap so hierarchy islands do not weld together

Host notes
----------
- The Mount section exposes the current filesystem root. TRUEOSFS mount labels,
  launch vFiles, terminal leases, and Shell2 lifecycle hooks do not exist here.
- Zoom emits the same OSC 777 request used by TRUEOS. Terminals that do not
  implement it safely ignore it.
- 7z pack/unpack currently reports an unsupported-operation status in this host
  build; TRUEOS provides that operation through its archive API.

Delete and move behavior
------------------------
Confirmed delete moves an item to `.explorer-trash/`. Confirmed moves rename it
on disk. The graph is not re-scanned or re-laid out immediately: the old entry
stays in place as a non-interactive `🪦 removed` or `moved` marker until the next
reload, mount, parent, or depth refresh, and folder descendants disappear at
once.

Nix
---

    nix run github:t4ce/texplo/otheros

Install the `td` command:

    nix profile install github:t4ce/texplo/otheros
    td

Use `--refresh` after the branch changes:

    nix run --refresh github:t4ce/texplo/otheros

Official Nixpkgs packaging
--------------------------
The repository flake is the upstream build. A Nixpkgs package should use the
release tag, install the `td` binary, and set `meta.mainProgram = "td"`.
After termdir is accepted into Nixpkgs, the shorter commands will be:

    nix run nixpkgs#termdir
    nix profile install nixpkgs#termdir

These commands are documented here for the packaging target; until the Nixpkgs
change is merged, use the GitHub commands above.

Snap
----
The Snap Store package name is `termdir`:

    sudo snap install termdir
    termdir

The preferred short alias is `td`. Until the Store grants that automatic alias,
enable it locally with:

    sudo snap alias termdir td

For removable drives, connect the optional interface:

    sudo snap connect termdir:removable-media

See `docs/SNAP_STORE.md` for confinement limits and the release procedure.
