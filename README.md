# naml

AppImages installer updater and manager for Linux.

**Documentation: <https://andunieee.github.io/naml/>**, with a user guide and
diagrams of how everything works. Its sources are in [`website/`](website/).

## what it does

- **Installs and integrates apps.** AppImages, static binaries and archives
  (tar.*, zip) are installed under your home directory, no root needed. Each
  graphical app gets an entry in the applications menu (also called the
  application launcher, or in freedesktop terms a *desktop entry*), plus its
  icon, and a command in `~/.local/bin` (a symlink to the installed
  executable). Use `--no-desktop` / `--no-bin` to skip either one.
- **Updates apps**, including through the
  [AppImageUpdate](https://github.com/AppImageCommunity/AppImageUpdate)
  protocol: the update information embedded in an AppImage (`zsync|...` and
  `gh-releases-zsync|owner|repo|tag|pattern`, with the `latest`, `latest-pre`
  and `latest-all` tags) is honored, and only the changed blocks are downloaded
  using zsync delta transfers.
- **Reads AppImages without running them**: metadata, the `.desktop` file and
  the `.DirIcon` icon are extracted straight from the squashfs payload.
- **Rolls back** to the previous version, locks apps at a version, and can
  update in the background with a systemd user timer.

### how the menu integration works

It follows the freedesktop.org
[Desktop Entry Specification](https://specifications.freedesktop.org/desktop-entry-spec/latest/).
For each app, naml writes `~/.local/share/naml/launchers/naml-<author>-<id>.desktop`
(the `.desktop` file shipped in the AppImage, rewritten so `Exec=` and `Icon=`
point at the installed files, or a minimal generated one), links it from
`~/.local/share/applications`, copies the icon into naml's data directory,
and runs `update-desktop-database` so the desktop environment picks it up.
The author part (`telegram.org`, `github.com-<owner>`, an npub, `local`)
says where the app came from. Removing the app removes those files.

### installing for everyone

Add `--system` (or `--global`) to any command to work on the apps installed
for every user of the machine instead of just your own. naml asks for root
with `sudo` (or `doas`) by itself.

```sh
naml --system install https://telegram.org/dl/desktop/linux
naml --system list
naml --system autoupdate enable   # a system timer that updates them as root
```

These apps live in `/opt/naml`, their database in `/var/lib/naml`, their
commands in `/usr/local/bin` and their launchers in
`/usr/local/share/applications`. An app you also install for yourself takes
precedence over the system one.

`naml move <app> system` moves one of your apps to the ones for everyone, and
`naml move <app> user` brings one back, keeping its name, rollback and
everything changed by hand. Run it as yourself; it asks for root when needed. `naml --system run <app>` runs the app as
you, not as root. The system timer only runs a naml binary that only root
can change, so put naml in `/usr/local/bin` before you enable it.

### ids and authors

Every app has an id and an author, written `<author>/<id>`. For apps from the
catalog the author is the publisher's npub. For everything else it is where
the app came from: `github.com/<owner>` for GitHub repositories, the website's
domain for urls (`telegram.org/telegram`) and `local` for files. Ids only
have to be unique per author; when two installed apps share one, use the
`<author>/<id>` form. For downloads, naml takes the id from the archive's
top-level directory or the file name, and `--id` picks it yourself.

### changing apps by hand

naml keeps these changes, so updates, rollbacks and reinstalls redo them:

```sh
naml rename <app> <name>          # your own name; also renames the command
naml set <app> exec Telegram      # run another file of the archive (no value: let naml pick)
naml set <app> id telegram        # a new id, for apps that didn't come from the catalog
naml desktop create <app>         # add it to the applications menu even if it isn't graphical
naml desktop set <app> Name "Telegram Desktop"
naml desktop set <app> Categories "Network;Chat;"
naml desktop unset <app> Comment  # remove a key
naml desktop edit <app>           # edit the launcher in $VISUAL / $EDITOR
naml desktop show <app>
naml desktop reset <app>          # undo every change to the launcher
naml desktop remove <app>         # take it out of the menu
```

## publishing your app

Maintainers can announce releases automatically from CI with the GitHub
Action in this repository; see
[docs/publish-from-github-actions.md](docs/publish-from-github-actions.md)
(written to be read by coding agents). To build AppImages with a proper
desktop entry and update information in the first place, see
[docs/build-appimages.md](docs/build-appimages.md).

## installation

Prebuilt static binaries are published on the
[releases page](https://github.com/andunieee/naml/releases). They are built
against musl, so they run on any linux distribution without extra libraries.

```sh
arch=$(uname -m)
case "$arch" in
  x86_64) target=x86_64-unknown-linux-musl ;;
  aarch64|arm64) target=aarch64-unknown-linux-musl ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac

mkdir -p ~/.local/bin
curl -fsSL "https://github.com/andunieee/naml/releases/latest/download/naml-$target" \
  -o ~/.local/bin/naml
chmod +x ~/.local/bin/naml
```

Make sure `~/.local/bin` is on your `PATH`, then check that it works:

```sh
naml --version
```

## shell completions

`naml completions <shell>` prints a completion script for bash, zsh, fish,
elvish or powershell. For example:

```sh
# bash
naml completions bash > ~/.local/share/bash-completion/completions/naml

# zsh (make sure ~/.zfunc is in your $fpath)
mkdir -p ~/.zfunc
naml completions zsh > ~/.zfunc/_naml

# fish
naml completions fish > ~/.config/fish/completions/naml.fish
```

Open a new shell afterwards. Completions cover commands, flags and fixed
values, and app names/ids are completed dynamically from the installed apps.
The generated scripts call `naml` again while completing, so keep `naml` on
your `PATH` and regenerate the script after upgrading `naml`.
