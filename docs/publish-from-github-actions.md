# publishing an app from GitHub Actions

This guide is written for coding agents setting up release automation for a
project. Follow it literally; every step is needed.

Goal: when a release is built, announce it in the naml catalog so users get it
with `naml install` and `naml update`.

If the project doesn't produce an AppImage with a desktop entry and update
information yet, read [build-appimages.md](build-appimages.md) first.

## what you need from the human

1. A **secret key** (`nsec1...`) for the identity that owns the app. It is the
   publisher's identity: people install `<npub>/<id>`, so it must stay the same
   across releases. Never print it or commit it. Ask the human to add it as a
   repository secret named `NAML_SECRET_KEY` (Settings → Secrets and variables
   → Actions). Any key generator that produces an `nsec` works.
2. A stable **app id** (`id` input), lowercase with dashes, e.g. `my-app`.
   Changing it later publishes a different app.

## what the workflow must do

1. Build the release file: an AppImage, a static binary or an archive.
2. Upload it somewhere with a stable public https URL (usually a GitHub
   release asset: `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>`).
3. Run the `andunieee/naml` action with that URL.

naml downloads (or hashes `file:`) the file and records its sha256, size,
architecture and, for AppImages, the name, summary, version, categories and
update information embedded in the AppImage. Anything you pass explicitly
overrides what it detects.

## minimal workflow

```yaml
name: release
on:
  push:
    tags: ["v*"]

permissions:
  contents: write

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # replace with the project's real build; it must produce one file
      - run: ./build.sh   # -> dist/MyApp-x86_64.AppImage

      - uses: softprops/action-gh-release@v2
        with:
          files: dist/MyApp-x86_64.AppImage

      - uses: andunieee/naml@v1
        with:
          secret-key: ${{ secrets.NAML_SECRET_KEY }}
          id: my-app
          file: dist/MyApp-x86_64.AppImage
          urls: https://github.com/${{ github.repository }}/releases/download/${{ github.ref_name }}/MyApp-x86_64.AppImage
```

Pass `file` whenever the local file exists: it avoids depending on the
download URL being live yet. The hash is computed from the local file, so the
uploaded asset must be byte-identical to it.

## one announcement per architecture

An app id has one announcement; publishing again replaces the previous one.
Publish a single file per `id`. If you ship x86_64 and aarch64 builds, either
use separate ids (`my-app-x86_64`, `my-app-aarch64`) or point both at one
multi-arch archive. Do not run the action twice with the same `id`.

## inputs

| input | meaning |
| --- | --- |
| `secret-key` | required, the publisher's nsec |
| `urls` | required, download urls one per line, mirrors after the first |
| `file` | local file to hash instead of downloading the first url |
| `id` | stable app identifier; defaults to one derived from the file name |
| `name`, `summary`, `description` | display text; AppImages provide name/summary themselves |
| `app-version` | version string; AppImages may provide it |
| `icon`, `web`, `license` | icon url, homepage, SPDX license |
| `format` | `appimage`, `binary`, `tar.gz`, `tar.xz`, `tar.zst`, `tar.bz2`, `tar`, `zip` (detected) |
| `exec` | for archives, path of the executable inside it |
| `update` | AppImageUpdate information, e.g. `gh-releases-zsync\|owner\|repo\|latest\|MyApp-*x86_64.AppImage.zsync` |
| `arch` | one per line, `x86_64` / `aarch64` (detected from ELF files) |
| `tags` | categories or keywords, one per line |
| `dry-run` | `true` prints the announcement without publishing |
| `naml-version` | naml release to use, default `latest` |

## checklist before finishing

- [ ] `NAML_SECRET_KEY` exists as a repository secret (ask the human; you can't verify its value).
- [ ] The workflow first runs with `dry-run: true` (for example on a branch or `workflow_dispatch`) and the printed announcement has the right name, version, arch and url.
- [ ] The url in `urls` is public and serves the exact bytes of `file`.
- [ ] `id` is fixed and not derived from the version or tag.
- [ ] Publishing runs after the asset upload step.

## without the action

The action only installs the naml binary and runs two commands, which you can
use directly in any CI:

```sh
naml --yes login "$NAML_SECRET_KEY"
naml --yes publish --id my-app --file MyApp.AppImage https://example.com/MyApp.AppImage
```

Prebuilt binaries: `https://github.com/andunieee/naml/releases/latest/download/naml-x86_64-unknown-linux-musl`
(or `aarch64-...`).
