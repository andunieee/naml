+++
title = "Moving apps"
description = "How naml move hands an app between your space and the system's without root touching your home."
weight = 80

[extra]
lead = "Each side's database is only opened by its owner. Your naml does your side and asks a naml running as root for the system side, handing the app over as json."
+++

## To the system

```mermaid
sequenceDiagram
  participant u as naml (you)
  participant r as naml --system (root, through sudo)
  u->>u: find the app in your database
  u->>r: __receive, with the app's record and paths as json on stdin
  r->>r: check that the files belong to you
  r->>r: copy them to /opt/naml, readable by everyone
  r->>r: write its launcher, icon and command; save it
  r-->>u: done
  u->>u: remove your copy
```

## To you

```mermaid
sequenceDiagram
  participant u as naml (you)
  participant r as naml --system (root, through sudo)
  u->>r: __send app
  r-->>u: the app's record and paths, as json on stdout
  u->>u: copy the files from /opt/naml (readable by everyone)
  u->>u: write its launcher, icon and command; save it
  u->>r: --system remove app
  r->>r: remove the system copy
```

## What carries over

The whole record moves: the app's name, its id, its lock, its hand-picked
executable, its launcher edits and its catalog announcement. The previous
version is copied too, so rollbacks still work. The launcher, icon and
command are created fresh on the new side, and removed from the old one.

## Safety

- If the receiving side fails, nothing is removed from the sending side. If
  removing the system copy fails after a move to you, naml says so and tells
  you how to remove it.
- `__receive` only copies directories owned by the user who ran `sudo`, so
  being allowed to run naml as root doesn't let anyone copy root-only files
  into world-readable `/opt`.
- `naml move` refuses to run under `sudo` itself, as root's own apps would
  then be "your" side.
- The `__receive` and `__send` commands are hidden: they are only meant to be
  called by `naml move`.
