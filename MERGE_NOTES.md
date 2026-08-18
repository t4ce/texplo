# Texplo merged handoff head

This tree preserves the uploaded current Texplo feature sources and restores the typed TRUEOS terminal handoff lifecycle from the archived `6775e80...` / `8189c85...` implementation.

## Default TRUEOS build

On TRUEOS/ZKVM the default binary compiles only `src/handoff_demo.rs`; the filesystem-backed graph/action/UI modules are excluded from the target entirely. This makes the next bare-metal run a terminal handoff test rather than a VFS/POSIX-directory test.

Controls:

- `Esc`: restore Crossterm state, release the terminal lease to Shell2, and park for re-entry.
- Shell2 `tui`: request re-entry; the same process obtains the next lease and redraws.
- `R`: repaint.
- `Ctrl-Q`: restore, release, and exit.

## Preserved full Texplo

The uploaded files are retained as the authoritative feature state:

- `actions.rs`
- `graph_view.rs`
- `layout.rs`
- `menu_view.rs`
- `minimap.rs`
- `screen.rs`

The former `main.rs` logic now lives in `src/full_app.rs`, with the typed lease/session machinery restored. Its TRUEOS ordering is also corrected so `App::new()` completes before `terminal_initial_lease()` is requested.

Native builds continue to run the full app. TRUEOS/ZKVM uses the isolated probe unless `full-trueos-app` is explicitly enabled.

To target the full filesystem-backed TRUEOS app later, explicitly build with `--features full-trueos-app` after the typed VFS path is intentionally ready. A plain build — even one using `--no-default-features` — remains the isolated terminal probe, so packaging cannot accidentally fall back into POSIX directory imports.
