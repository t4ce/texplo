# Snap Store release

The registered Snap Store package name is `termdir`, owned by the `t4ce`
publisher account. The human-facing application name is also **termdir**.

The snap is strictly confined. It can access ordinary files in the user's home
directory through the `home` interface. The optional `removable-media`
connection extends this to mounted removable drives:

```sh
sudo snap connect termdir:removable-media
```

Strict confinement deliberately does not expose the complete host filesystem
or hidden files throughout the real home directory. Use the Nix, Cargo, or
distribution-native package when unrestricted host filesystem navigation is
required.

## Command names

The guaranteed command is:

```sh
termdir
```

The manifest requests `td` as an automatic alias. Store aliases require review.
Until that alias is granted, a user can enable it locally:

```sh
sudo snap alias termdir td
```

## Build and test

Build without copying the repository's `target/` directory into Snapcraft:

```sh
snapcraft pack --destructive-mode --output snap
sudo snap install --dangerous ./snap/termdir_0.14.0_amd64.snap

termdir
sudo snap alias termdir td
td
```

The 800 px canonical logo is `assets/logo.png`. The Store-compatible 512 px
derivative is `snap/gui/icon.png`; regenerate it from the canonical logo rather
than repeatedly resizing the derivative. Update the listing metadata and icon
from the reviewed package with:

```sh
snapcraft upload-metadata snap/termdir_0.14.0_amd64.snap --force
```

## Publish progressively

Upload the locally reviewed package to `edge` first:

```sh
snapcraft upload --release=edge snap/termdir_0.14.0_amd64.snap
```

Test that exact Store revision on another Ubuntu system before promoting it to
`candidate` and then `stable`. The `td` automatic alias and any future elevated
interfaces must be approved by the Snap Store before they become automatic.
