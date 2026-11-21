+++
title = "Names, ids and authors"
description = "How apps are identified, and how to refer to them in commands."
weight = 20

[extra]
lead = "Every installed app has an id, an author, and optionally a name of your own. Commands accept any of them."
+++

## The pieces

| | example | where it comes from |
|---|---|---|
| **id** | `helix`, `telegram` | the publisher's `d` tag for catalog apps; for the others, the archive's top-level directory, the file name or the repository name, or `--id` |
| **author** | `npub1…`, `github.com/helix-editor`, `telegram.org`, `local` | the publisher's public key for catalog apps; for the others, where the app came from |
| **name** | `hx` | yours, set with `--name` or `naml rename`; optional |
| **slug** | `telegram`, `telegram-2` | naml's internal handle, used for file names: the name if there is one, otherwise the id, with a suffix when it is already taken |

`<author>/<id>` is what `naml info` shows as the app's id:

```text
          name  telegram
            id  telegram.org/telegram
```

The author of an app that didn't come from the catalog is:

- `github.com/<owner>` for GitHub repositories;
- the url's domain (without `www.`) for urls;
- `local` for local files.

## Referring to apps

Wherever a command takes an app, you can give:

1. its **name**;
2. its **slug**;
3. its **id**, when no other installed app has the same one;
4. `<author>/<id>`, which always works.

```sh
naml info hx
naml info helix
naml info github.com/helix-editor/helix
```

Shell [completions](@/docs/getting-started/installation.md#shell-completions)
offer the names and ids of your installed apps, and the qualified form when
an id is ambiguous.

## Renaming

```sh
naml rename telegram tg   # or: naml name telegram tg
naml rename tg            # drop the name again
```

The command in `~/.local/bin` follows the name, and so do the app's
directory and launcher, so nothing internal keeps the old name.

## Changing the id

The id of an app that didn't come from the catalog can be changed:

```sh
naml set app id telegram
```

If the app had no name of its own, its displayed name follows the id too.
Catalog apps keep the id their publisher gave them, since it is how updates
find them.
