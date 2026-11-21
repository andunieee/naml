+++
title = "Publishing apps"
description = "Announce your app in the catalog, by hand or from GitHub Actions."
weight = 80

[extra]
lead = "Anyone with a nostr identity can announce an app: naml hashes the file, reads what it can from it, and publishes a signed event."
+++

## With naml publish

```sh
naml login nsec1…    # or a bunker:// signer
naml publish https://github.com/me/app/releases/download/v1.2.0/App-x86_64.AppImage \
  --id my-app --summary "does things" --web https://example.com --license MIT
```

naml downloads the file (or hashes a local copy given with `--file`), and
records its sha256 and size. It detects what it can:

- the **format**, from the file's contents;
- the **architecture**, from the ELF header of AppImages and binaries;
- for AppImages, the **name**, **summary**, **version** and **categories**
  from the embedded desktop entry, and the **update information** from the
  ELF section that holds it.

Anything you pass explicitly wins over what naml detects. `--dry-run` prints
the event without publishing it. Publishing again with the same `--id`
replaces the announcement, and that is how users get the update.

| option | what it sets |
|---|---|
| `<urls>…` | where the file can be downloaded, mirrors after the first |
| `--file <path>` | hash this local file instead of downloading the first url |
| `--id <id>` | the app's id (its `d` tag); keep it the same across releases |
| `--name`, `--summary`, `--description` | how the app is shown |
| `--app-version <version>` | its version |
| `--icon <url>` | an icon, for apps that don't embed one |
| `--web <url>`, `--license <spdx>` | homepage and license |
| `--format <format>` | `appimage`, `binary`, `tar.gz`, `tar.xz`, `tar.zst`, `tar.bz2`, `tar` or `zip` |
| `--exec <path>` | for archives, the executable inside it |
| `--update <info>` | AppImageUpdate update information |
| `--arch <arch>` | supported architectures, repeatable |
| `-t`, `--tag <tag>` | categories or keywords, repeatable |
| `--dry-run` | print instead of publishing |

For CI, set the secret key in `NAML_SECRET_KEY` instead of logging in, so
nothing is stored.

## From GitHub Actions

The repository is also a GitHub Action that runs `naml publish` for you:

```yaml
- uses: andunieee/naml@v1
  with:
    secret-key: ${{ secrets.NAML_SECRET_KEY }}
    urls: https://github.com/${{ github.repository }}/releases/download/${{ github.ref_name }}/MyApp-x86_64.AppImage
    id: my-app
```

Two detailed guides, written to be followed by coding agents as well as
people, live in the repository:

- [Publishing from GitHub Actions](https://github.com/andunieee/naml/blob/master/docs/publish-from-github-actions.md):
  the workflow, one announcement per architecture, every input.
- [Building AppImages that naml can install and update](https://github.com/andunieee/naml/blob/master/docs/build-appimages.md):
  the AppDir layout, the desktop entry, update information.

## What users see

Users install your app as `<your npub>/<id>`, or just `<id>` when no one else
uses it. [The catalog protocol](@/docs/internals/catalog.md) documents the
event format, if you'd rather publish with other tools.
