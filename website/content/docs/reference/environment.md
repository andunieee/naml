+++
title = "Environment variables"
description = "The environment variables naml reads."
weight = 20
+++

| variable | effect |
|---|---|
| `NAML_SECRET_KEY` | an `nsec` secret key to sign with, taking precedence over `naml login`; meant for CI, where nothing should be stored |
| `VISUAL`, `EDITOR` | the editor for `naml desktop edit`, in that order, falling back to `vi`; may include arguments |
| `XDG_DATA_HOME` | where `naml/` and `applications/` live (default `~/.local/share`) |
| `XDG_CONFIG_HOME` | where the systemd user units go (default `~/.config`) |
| `XDG_BIN_HOME` | where commands are linked (default `~/.local/bin`) |
| `COMPLETE` | set by the completion scripts when they call naml to complete a command line |
| `SUDO_UID`, `SUDO_USER` | set by `sudo`; used by [`naml move`](@/docs/internals/moving.md) to check whose files are handed over, and to refuse running under `sudo` itself |

The `XDG_*` variables only affect your own apps. Apps installed with
`--system` always use the [system locations](@/docs/internals/files.md#apps-for-everyone).
