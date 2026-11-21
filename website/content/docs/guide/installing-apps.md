+++
title = "Installing apps"
description = "Every kind of source naml installs from, and the install options."
weight = 10

[extra]
lead = "`naml install` (or `naml i`) takes one or more targets: something from the catalog, a GitHub repository, a url or a local file."
+++

## From the catalog

Apps announced in the [catalog](@/docs/guide/catalog.md) are installed by
their id (the publisher's `d` tag):

```sh
naml install helix
```

If more than one publisher uses that id, naml lists them and asks you to pick
one with the `<author>/<id>` form, where the author is an `npub`, an
`nprofile`, a hex public key or a NIP-05 address:

```sh
naml install npub1…/helix
naml install alice@example.com/helix
naml install naddr1…          # a nostr address code works too
```

A bare id is looked up in the local catalog, which is refreshed automatically
when it is more than a day old. A qualified `<author>/<id>` is always fetched
fresh from the catalog servers. Catalog apps carry the file's sha256, and the
download is checked against it. Apps published only for other architectures
are refused.

## From a GitHub repository

```sh
naml install https://github.com/owner/repo
naml install github.com/owner/repo           # the scheme is optional
naml install https://github.com/owner/repo/releases/tag/v2.1.0
```

naml takes the newest stable release (or the newest prerelease, for projects
that only publish those), or the release whose page you linked. From its
assets it picks the one that fits this machine:

1. AppImages first, then archives (`.tar.*`, `.zip`), then bare executables
   whose name says `linux`;
2. assets naming your architecture (`x86_64`/`amd64`/`x64`,
   `aarch64`/`arm64`) over ones naming none; ones naming another architecture
   or another system (macOS, Windows, …) are never picked;
3. archives that don't say `linux` and don't name an architecture are
   skipped, as they are usually sources.

When several assets fit equally well and naml is running in a terminal, it
asks which one you want. Later updates follow the repository's newest
releases, picking the asset shaped like the one you installed.

A direct download link (`…/releases/download/<tag>/<file>`) is installed as a
plain url instead.

## From a url

```sh
naml install https://telegram.org/dl/desktop/linux
```

naml downloads the file, follows redirects, and detects the format from the
file's contents, so the url doesn't need to end in `.AppImage` or `.tar.gz`.
Updates come from the AppImage's own update information if it has some, or
from watching the url for changes (its `ETag` or `Last-Modified`).

## From a local file

```sh
naml install ./MyApp-x86_64.AppImage
naml install ~/Downloads/tool.tar.gz
naml install file:///opt/builds/tool
```

The file is copied, so you can delete the original afterwards.

## What can be installed

| format | how it's recognized | what naml does |
|---|---|---|
| AppImage | ELF file with the `AI` magic | stored as `<name>.AppImage`; desktop entry and icon read from inside it |
| static binary | any other ELF file | stored as `<name>` |
| `tar.gz`, `tar.xz`, `tar.zst`, `tar.bz2`, `tar` | compression and tar magic | extracted; a lone top-level directory is stripped; the main executable is picked |
| `zip` | zip magic | same as tarballs |

For archives, naml picks the main executable among the programs inside: it
prefers files named like the app, files in a `bin/` directory, shallow
files, and bigger files, and avoids helpers such as updaters, uninstallers,
crash reporters and `.sh` scripts. If it gets it wrong,
[`naml set <app> exec`](@/docs/guide/customizing.md#choosing-the-executable)
fixes it for good. The details are in
[How installation works](@/docs/internals/installation-process.md).

AppImages and binaries built for another architecture are refused.

## Options

| option | what it does |
|---|---|
| `-n`, `--name <name>` | a [name of your own](@/docs/guide/names-and-ids.md) for the app; it also names its command |
| `--id <id>` | the id for an app from a url, a file or GitHub (by default one is made from the archive's top directory or the file name) |
| `-f`, `--force` | reinstall an app that is already installed; your [hand edits](@/docs/guide/customizing.md) are kept |
| `--no-desktop` | don't add it to the applications menu |
| `--no-bin` | don't link a command in `~/.local/bin` |
| `--system` | install it [for every user](@/docs/guide/system-wide.md) |

Several targets can be installed at once (`naml install a b c`); `--name`
and `--id` only work with one.

## What you get

After an install, the app has:

- its files in `~/.local/share/naml/apps/<slug>/`;
- a launcher in the applications menu, if it looks graphical (an AppImage,
  an app that ships a `.desktop` file, or one with an icon), or if you
  [ask for one](@/docs/guide/customizing.md#launchers);
- a command `~/.local/bin/<name>`, a symlink to its executable.

See [Files and directories](@/docs/internals/files.md) for the full list.
