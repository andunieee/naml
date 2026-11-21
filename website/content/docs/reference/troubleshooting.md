+++
title = "Troubleshooting"
description = "Common problems and how to fix them."
weight = 30
+++

## The command runs the wrong program

naml picked the wrong executable of an archive, for example an updater
instead of the app:

```sh
naml info <app>                  # look at "executable"
naml set <app> exec <the right one>
```

naml lists the programs in the app if the name doesn't match.

## The app isn't in the applications menu

naml only makes launchers for apps that look graphical. Ask for one:

```sh
naml desktop create <app>
naml desktop set <app> Name "The App"
```

If the launcher exists but the menu doesn't show it, log out and in again;
some desktop environments only rescan on login.

## "already exists and isn't ours, not linking the command"

There is already a file at `~/.local/bin/<name>` that naml didn't create.
naml never overwrites it. Rename the app (`naml rename <app> <other>`), or
remove that file and run `naml rename <app> <name>` again to link the command.

## The id is `app`, or not what I expected

The download didn't say what the app is called. Change it:

```sh
naml set app id the-real-name
naml rename app the-real-name    # if you also want the command renamed
```

Next time, pass `--id` to `naml install`.

## "is ambiguous, it could be any of"

Two installed apps share that id. Use one of the `<author>/<id>` forms naml
prints, or give one of them a name with `naml rename`.

## Search finds nothing

The catalog may be stale or the catalog servers unreachable:

```sh
naml sync
naml servers list
```

## "can't open … (is another naml running?)"

The database is locked by another naml, possibly the
[automatic update](@/docs/guide/updating.md#automatic-updates). Wait for it to
finish.

## A bad update

```sh
naml rollback <app>
naml lock <app>
```

## Apps for everyone ask for a password

That's `sudo`: only root can change them. `naml --system run <app>` and the
apps' commands themselves run as you without asking.
