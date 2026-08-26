# termdir TRUEOS branch

The `true` branch is the TRUEOS-native termdir application. The full terminal directory explorer is the default and only application path.

termdir directly uses the TRUEOS terminal lease, clock, archive, collections, and asynchronous TRUEOSFS interfaces. Host filesystem, process, and terminal fallbacks are intentionally absent.

Build the application through the TRUEOS Blueprint toolchain:

```sh
cd ../..
cargo bp termdir
```

`Esc` restores Crossterm state and releases the terminal lease to Shell2. Re-entering `td` returns the lease to the same application, while `Ctrl-Q` releases it and exits.
