+++
title = "Quick start"
description = "A first session with naml: install, run, update and remove an app."
weight = 30

[extra]
lead = "Five minutes with naml, from the first install to the first update."
+++

## Install an app

Point naml at a GitHub repository that publishes Linux releases:

```sh
naml install https://github.com/obsidianmd/obsidian-releases
```

naml asks GitHub for the newest release, picks the asset that fits your
machine (it prefers AppImages, then archives, then bare binaries for your
CPU), downloads it, and integrates it:

```text
:: Obsidian-1.8.9.AppImage from obsidianmd/obsidian-releases v1.8.9
✓ installed obsidian-releases 1.8.9 (run it with `obsidian-releases`)
```

It's now in your applications menu with its icon, and on your `PATH`.

## Give it a better name

The id came from the repository name. Give the app a name of your own:

```sh
naml rename obsidian-releases obsidian
obsidian   # the command follows the name
```

## Look at it

```sh
naml list
naml info obsidian
```

`info` shows where the app lives, what it runs, its launcher, its command and
how it is updated. The [files page](@/docs/internals/files.md) explains every
path.

## Update

```sh
naml update --check   # just tell me
naml update           # update everything that changed
```

AppImages with update information download only the blocks that changed.
If the new version misbehaves:

```sh
naml rollback obsidian   # back to the previous version
naml lock obsidian       # and stay there for now
```

## Remove

```sh
naml remove obsidian
```

This removes the app, its previous version, its launcher, its icon and its
command. Nothing is left behind.

## Next

- [Installing apps](@/docs/guide/installing-apps.md) covers every kind of
  source and the install options.
- [Customizing apps](@/docs/guide/customizing.md) shows how to fix an app
  whose executable or launcher naml guessed wrong.
- [How installation works](@/docs/internals/installation-process.md) walks
  through what happens behind `naml install`.
