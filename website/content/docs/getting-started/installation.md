+++
title = "Installing naml"
description = "Get the naml binary, put it on your PATH and set up shell completions."
weight = 20

[extra]
lead = "naml is a single static binary. Download it, put it on your PATH, done."
+++

## Prebuilt binaries

Static binaries for `x86_64` and `aarch64` are published on the
[releases page](https://github.com/andunieee/naml/releases). They are built
against musl, so they run on any distribution without extra libraries.

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

Make sure `~/.local/bin` is on your `PATH` (most distributions add it when it
exists), then check:

```sh
naml --version
```

### For every user

If you also want to [install apps for everyone](@/docs/guide/system-wide.md)
and update them automatically as root, put naml where only root can change it:

```sh
sudo install -m 755 ~/.local/bin/naml /usr/local/bin/naml
```

## From source

naml is written in Rust (edition 2024):

```sh
git clone https://github.com/andunieee/naml
cd naml
cargo build --release
install -m 755 target/release/naml ~/.local/bin/naml
```

## Shell completions

`naml completions <shell>` prints a completion script for `bash`, `zsh`,
`fish`, `elvish` or `powershell`:

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
values, and the names and ids of your installed apps are completed
dynamically. The scripts call `naml` again while completing, so keep it on
your `PATH`, and regenerate the script after upgrading naml.

## What naml needs

Nothing beyond a Linux kernel and a home directory. A few optional tools are
used when they are there:

| tool | used for |
|---|---|
| `update-desktop-database` | refreshing the menu cache after a launcher changes |
| `systemctl` | [automatic updates](@/docs/guide/updating.md#automatic-updates) with a systemd timer |
| `sudo` or `doas` | managing apps [installed for everyone](@/docs/guide/system-wide.md) |
| `$VISUAL` / `$EDITOR` | [`naml desktop edit`](@/docs/guide/customizing.md) |
