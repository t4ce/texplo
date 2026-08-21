# Texplo TRUEOS branch

The `true` branch is the TRUEOS-native Texplo application. The full Terminal Directory Explorer is the default and only application path; it does not require a Texplo feature flag or target-specific dependency selection.

Texplo directly uses the TRUEOS terminal lease, clock, archive, collections, and asynchronous TRUEOSFS interfaces. Host filesystem, process, and terminal fallbacks are intentionally absent.

Build the application through the TRUEOS Blueprint toolchain:

```sh
cd ../TRUEOS-Blueprints
cargo bp texplo
```

`Esc` restores Crossterm state and releases the terminal lease to Shell2. Re-entering `tde` returns the lease to the same application, while `Ctrl-Q` releases it and exits.
