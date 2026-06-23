# naml

nostr app manager for linux

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
values; app names are not completed dynamically.
