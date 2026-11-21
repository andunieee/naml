+++
title = "Customizing apps"
description = "Choose another executable, change an app's id, and edit its launcher."
weight = 50

[extra]
lead = "When naml guesses something wrong, or you just want it different, change it. naml keeps your changes across updates, rollbacks and reinstalls."
+++

## Choosing the executable

Archives often hold more than one program. If naml picked the wrong one
(an updater, a helper), name the right one:

```sh
naml set telegram exec Telegram          # a file name, if it is unique
naml set tool exec bin/tool-gui          # or a path inside the app directory
naml set tool exec                       # no value: let naml pick again
```

If the name doesn't match anything, naml lists the programs it found:

```text
error: there is no nope in the app, these could be run:
  Telegram
  Updater
```

The command in `~/.local/bin` and the launcher are pointed at the new
executable right away. Updates keep using it as long as the new version still
contains it; when it's gone, naml warns and picks again. AppImages and static
binaries are a single file, so there is nothing to choose.

## Changing the id

```sh
naml set app id telegram
```

See [Names, ids and authors](@/docs/guide/names-and-ids.md#changing-the-id).

## Launchers

A launcher is the app's entry in the applications menu (a *desktop entry*, in
freedesktop terms). naml makes one for apps that look graphical. These
commands manage it (`naml launcher` is an alias of `naml desktop`):

```sh
naml desktop show <app>                         # print it
naml desktop create <app>                       # make one even if the app doesn't look graphical
naml desktop set <app> Name "Telegram Desktop"  # set a key
naml desktop set <app> Categories "Network;Chat;"
naml desktop set <app> Terminal true
naml desktop set <app> Name                     # no value: drop your change to it
naml desktop unset <app> Comment                # remove a key
naml desktop edit <app>                         # edit it in $VISUAL or $EDITOR
naml desktop reset <app>                        # undo every change
naml desktop remove <app>                       # take it out of the menu
```

`create` makes a minimal entry with just the app's name, its executable and
its icon (if it has one). You can then `set` whatever else you need. `set`,
`unset` and `edit` create the launcher if there isn't one yet.

Only the `[Desktop Entry]` group can be changed. The keys are the ones of the
[Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry-spec/latest/recognized-keys.html):
`Name`, `GenericName`, `Comment`, `Categories`, `Keywords`, `Terminal`,
`StartupWMClass`, `MimeType` and so on, including localized ones such as
`Name[de]`. `X-Naml-Slug` is naml's own and can't be changed.

`edit` opens the launcher in your editor and keeps the keys you changed,
added or deleted. The editor can be a command with arguments, like
`VISUAL="code --wait"`.

## How your changes are kept

naml stores what you changed, not the resulting file. Whenever it writes the
launcher again (after an update, a rollback, a rename, a reinstall with
`--force`, or a [move](@/docs/guide/system-wide.md#moving-apps)), it builds
the entry from the app as usual and then applies your changes on top. A
`naml info` lists them:

```text
launcher edits  Categories=Network;Chat;, -Comment, Name=Telegram Desktop
```

`Key=value` sets a key, `-Key` removes it. `naml desktop reset` forgets them
all.
