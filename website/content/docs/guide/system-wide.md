+++
title = "Installing for everyone"
description = "Apps for every user of the machine with --system, and moving apps between your space and the system's."
weight = 60

[extra]
lead = "Add `--system` (or `--global`) to any command to work on the apps installed for every user of the machine instead of just your own."
+++

## Installing for every user

```sh
naml --system install https://telegram.org/dl/desktop/linux
naml --system list
naml --system update
naml --system desktop set telegram Categories "Network;Chat;"
```

Everything works as it does for your own apps, but in system locations:

| what | your apps | apps for everyone |
|---|---|---|
| app files | `~/.local/share/naml/apps/` | `/opt/naml/apps/` |
| previous versions | `~/.local/share/naml/previous/` | `/opt/naml/previous/` |
| icons and launchers | `~/.local/share/naml/{icons,launchers}/` | `/opt/naml/{icons,launchers}/` |
| database | `~/.local/share/naml/naml.redb` | `/var/lib/naml/naml.redb` |
| commands | `~/.local/bin/` | `/usr/local/bin/` |
| menu entries | `~/.local/share/applications/` | `/usr/local/share/applications/` |
| autoupdate units | `~/.config/systemd/user/` | `/etc/systemd/system/` |

[Files and directories](@/docs/internals/files.md) describes each of them.

## Root

Only root can change the apps for everyone. When you aren't root, naml runs
the same command again with `sudo` (or `doas` if there is no `sudo`), so
you'll be asked for your password. Everything naml writes there is made
readable by every user, whatever root's `umask` is.

`naml --system run <app>` is the exception: it starts the app's command from
`/usr/local/bin` as you, never as root.

If you install the same app both for yourself and for everyone, yours wins:
`~/.local/bin` comes before `/usr/local/bin` on the usual `PATH`, and
launchers in `~/.local/share/applications` take precedence over the system
ones with the same name.

## Moving apps

An app can move between your apps and the ones for everyone, keeping its
name, its previous version, its lock and everything you
[changed by hand](@/docs/guide/customizing.md):

```sh
naml move telegram system   # yours → everyone's (or: naml mv)
naml move telegram user     # everyone's → yours
```

Run `naml move` as yourself, not with `sudo`: it asks for root only for the
system side, so nothing owned by root ends up in your home.
[Moving apps](@/docs/internals/moving.md) shows the exchange step by step.
A move is refused when the app is already installed on the other side, or
when its name is taken there.

## Automatic updates for everyone

```sh
naml --system autoupdate enable
```

This installs a system timer that runs `naml --system update` as root. Since
root then runs the naml binary on its own, naml refuses unless that binary
and every directory above it can only be changed by root. Put naml in
`/usr/local/bin` first:

```sh
sudo install -m 755 ~/.local/bin/naml /usr/local/bin/naml
/usr/local/bin/naml --system autoupdate enable
```

## Limitations

- Shell completions only offer your own apps: the system database can only
  be opened by root.
- `naml list` without `--system` doesn't show the apps for everyone.
