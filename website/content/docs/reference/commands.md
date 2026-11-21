+++
title = "Commands"
description = "Every naml command, its arguments and flags."
weight = 10

[extra]
lead = "`naml [global options] <command> [arguments]`. Run `naml <command> --help` for the same information in the terminal."
+++

## Global options

These work with every command, before or after it.

| option | |
|---|---|
| `-y`, `--yes` | answer yes to every question |
| `-q`, `--quiet` | only print errors |
| `--system`, `--global` | work on the [apps installed for everyone](@/docs/guide/system-wide.md); asks for root with `sudo` |
| `-h`, `--help` | help for naml or a command |
| `-V`, `--version` | naml's version |

Wherever an `<app>` is expected, give its name, slug, id or
`<author>/<id>` ([details](@/docs/guide/names-and-ids.md#referring-to-apps)).

## Apps

| command | aliases | what it does |
|---|---|---|
| `install <target>…` | `i` | [install](@/docs/guide/installing-apps.md) from the catalog (`<id>`, `<author>/<id>`, `naddr`), a GitHub repository, an `https://` url or a file |
| `remove <app>…` | `rm`, `uninstall` | uninstall apps |
| `list` | `ls` | list installed apps |
| `info <app>` | | details of an installed or available app |
| `run <app> [args…]` | `x` | run an app with arguments |
| `update [app…]` | `up`, `upgrade` | [update](@/docs/guide/updating.md) apps, all of them by default |
| `rollback <app>` | `downgrade` | go back to the version before the last update |
| `lock <app>` | | keep an app at its current version |
| `unlock <app>` | | let an app update again |
| `name <app> [name]` | `rename` | give an app a name of your own, or drop it |
| `move <app> <system\|user>` | `mv` | [move an app](@/docs/guide/system-wide.md#moving-apps) between your apps and the ones for everyone |
| `autoupdate <enable\|disable\|status>` | | [update in the background](@/docs/guide/updating.md#automatic-updates) with a systemd timer |

### install

| option | |
|---|---|
| `-n`, `--name <name>` | a name of your own for the app |
| `--id <id>` | the id for an app from a url, a file or GitHub |
| `-f`, `--force` | reinstall if already installed (hand edits are kept) |
| `--no-desktop` | don't add it to the applications menu |
| `--no-bin` | don't link a command |

### update

| option | |
|---|---|
| `-c`, `--check` | only tell what can be updated |

## Changing apps

| command | what it does |
|---|---|
| `set <app> exec [path]` | the [program to run](@/docs/guide/customizing.md#choosing-the-executable), as a path inside the app or a file name; without one, naml picks it again |
| `set <app> id <id>` | the app's id, for apps that didn't come from the catalog |
| `desktop show <app>` | print the app's launcher |
| `desktop create <app>` | put the app in the menu, even if it doesn't look graphical |
| `desktop set <app> <key> [value]` | set a key of the launcher; without a value, drop your change to it |
| `desktop unset <app> <key>` | remove a key from the launcher |
| `desktop edit <app>` | edit the launcher in `$VISUAL` or `$EDITOR` |
| `desktop reset <app>` | undo every change made to the launcher by hand |
| `desktop remove <app>` | take the app out of the menu |

`launcher` is an alias of `desktop`.

## Catalog

| command | aliases | what it does |
|---|---|---|
| `search [query…]` | `s` | look for apps; `-r`/`--recommended` for only those your follows recommend |
| `sync` | | refresh the catalog from the catalog servers and recommendations |
| `login <key>` | | your identity: `nsec1…` or `bunker://…` (can publish), `npub1…` or a NIP-05 address (read-only) |
| `logout` | | forget your identity |
| `whoami` | | show your identity |
| `recommend list` | | what you recommend: installed catalog apps plus your changes |
| `recommend add <app>` | | recommend an app, installed or not |
| `recommend remove <app>` | | stop recommending an app |
| `recommend publish` | | publish your recommendations |
| `servers list` | | the catalog servers in use |
| `servers add <url>` | | add a catalog server |
| `servers remove <url>` | | remove one you added |
| `servers publish` | | publish your catalog servers |
| `publish <url>…` | | [announce an app](@/docs/guide/publishing.md) |

### publish

| option | |
|---|---|
| `--file <path>` | hash this local file instead of downloading the first url |
| `--id <id>` | the `d` tag (defaults to one from the file name) |
| `--name <name>` | display name |
| `--summary <text>` | one line about the app |
| `--description <text>` | the long description |
| `--app-version <version>` | its version |
| `--icon <url>` | url of an icon |
| `--web <url>` | homepage |
| `--license <license>` | license |
| `--format <format>` | `appimage`, `binary`, `tar.gz`, `tar.xz`, `tar.zst`, `tar.bz2`, `tar` or `zip` (detected by default) |
| `--exec <path>` | for archives, the path of the executable inside it |
| `--update <info>` | AppImageUpdate update information (read from the AppImage by default) |
| `--arch <arch>` | supported architectures, repeatable (detected from the file by default) |
| `-t`, `--tag <tag>` | categories or keywords, repeatable |
| `--dry-run` | print the event without publishing |

## Shell

| command | aliases | what it does |
|---|---|---|
| `completions <shell>` | `complete` | print a [completion script](@/docs/getting-started/installation.md#shell-completions) for `bash`, `zsh`, `fish`, `elvish` or `powershell` |
