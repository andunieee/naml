+++
title = "Introduction"
description = "What naml is and what it does."
weight = 10

[extra]
lead = "naml installs portable Linux apps (AppImages, static binaries and tar/zip archives), keeps them up to date, and hooks them into your desktop like any other app."
+++

## What naml does

Lots of Linux software ships as a single portable file: an AppImage, a static
binary, or an archive with the program inside. Such files run anywhere, but
they don't install themselves: you download them somewhere, `chmod +x` them,
maybe write a `.desktop` file by hand, and then forget to update them.

naml does those chores for you:

- **Installs** an app from the [catalog](@/docs/guide/catalog.md), a GitHub
  repository, any `https://` url or a local file. It figures out the format,
  picks the right release asset for your CPU, and unpacks archives.
- **Integrates** it: a launcher in the applications menu with the app's own
  icon and desktop entry, and a command in `~/.local/bin`.
- **Updates** it: through AppImageUpdate information with zsync delta
  downloads, through the publisher's catalog announcement, through GitHub
  releases, or by watching the url.
- **Keeps the previous version**, so a bad update is one
  [`naml rollback`](@/docs/guide/updating.md) away.
- **Remembers your changes**: a different executable, your own name for the
  app, edits to its launcher.

Everything goes under your home directory and needs no root. Apps can also be
[installed for every user](@/docs/guide/system-wide.md) of the machine.

## A short tour

```sh
# from a GitHub repository: the release asset that fits this machine
naml install https://github.com/obsidianmd/obsidian-releases

# from any url, giving it a name of your own
naml install https://telegram.org/dl/desktop/linux --name telegram

# from the catalog, by id
naml search editor
naml install helix

# see what you have, and keep it fresh
naml list
naml update
```

## How this documentation is organized

- [Getting started](@/docs/getting-started/_index.md): installing naml
  itself and a first session.
- [User guide](@/docs/guide/_index.md): task by task, everything naml can do.
- [How it works](@/docs/internals/_index.md): the install and update
  processes, the files naml touches, the database and the catalog protocol,
  with diagrams.
- [Reference](@/docs/reference/_index.md): every command and flag, every
  file and environment variable.
