+++
title = "Managing apps"
description = "Listing, inspecting, running and removing installed apps."
weight = 30

[extra]
lead = "The everyday commands: what's installed, what it is, run it, get rid of it."
+++

## Listing

```sh
naml list    # or: naml ls
```

One line per app: its name, its display name, its version, its format and
where it came from (the catalog address, or the url). Flags at the end say
when an app is `locked` or has a `rollback available`.

## Details

```sh
naml info <app>
```

For an installed app, `info` shows everything naml knows about it:

| row | meaning |
|---|---|
| `name` | how you refer to it (its name, or its slug) |
| `id` | `<author>/<id>` |
| `version`, `format`, `size`, `sha256` | of the installed file |
| `source` | the url it was downloaded from |
| `executable` | what runs, marked `(set by hand)` when you [chose it](@/docs/guide/customizing.md#choosing-the-executable) |
| `launcher` | its desktop entry |
| `launcher edits` | your [changes](@/docs/guide/customizing.md#launchers) to it |
| `command` | its link in `~/.local/bin` |
| `update info` | AppImageUpdate information, when it has some |
| `installed`, `updated` | dates |
| `locked`, `previous` | whether it is locked, and the version a rollback goes back to |

For an app that isn't installed, `info` describes its catalog announcement:
publisher, version, urls, hash, homepage, license, architectures, tags, who
recommends it and its description.

## Running

Installed apps are on your `PATH` and in the menu, but naml can also start
them, passing any arguments along:

```sh
naml run <app> [args…]    # or: naml x
naml run helix --health
```

naml replaces itself with the app, so signals and exit codes behave as if
you had run it directly.

## Removing

```sh
naml remove <app>…    # or: naml rm, naml uninstall
```

naml asks for confirmation (skip it with `-y`), then removes the app's files,
its previous version, its launcher, its icon and its command. A command in
`~/.local/bin` that isn't one of naml's links is never touched.

## Locking

```sh
naml lock <app>      # never update it
naml unlock <app>    # update it again
```

`naml update` skips locked apps, including when it runs in the
[background](@/docs/guide/updating.md#automatic-updates).

## Answering for scripts

Every command takes `-y`/`--yes` to answer yes to every question, and
`-q`/`--quiet` to print only errors.
