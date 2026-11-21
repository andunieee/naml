+++
title = "naml"

[extra]
lead = "Install, update and integrate portable Linux apps (<b>AppImages</b>, <b>static binaries</b> and <b>archives</b>) under your home directory, or for everyone on the machine. No root needed for your own apps, no packaging needed from publishers."
url = "docs/getting-started/introduction/"
url_button = "Get started"
repo_license = "MIT or Apache-2.0 —"

[[extra.list]]
title = "Install from anywhere"
content = "A <code>naml install</code> takes an app from the catalog, a GitHub repository, any https:// url or a local file, and picks the right asset for your machine."

[[extra.list]]
title = "Delta updates"
content = "AppImage update information is honored, and only the blocks that changed are downloaded with zsync. Plain urls and GitHub releases are followed too."

[[extra.list]]
title = "Real desktop integration"
content = "Each app gets a launcher in the applications menu with its icon, and a command on your <code>PATH</code>, read straight from the AppImage without running it."

[[extra.list]]
title = "Rollbacks and locks"
content = "The previous version is kept: go back to it with <code>naml rollback</code>, or keep an app where it is with <code>naml lock</code>."

[[extra.list]]
title = "Yours to adjust"
content = "Pick another executable, rename an app, or edit its launcher. naml remembers your changes across updates."

[[extra.list]]
title = "A catalog on nostr"
content = "Publishers announce apps with a signed event; you see what the people you follow recommend. No central server."
+++

```sh
naml install github.com/obsidianmd/obsidian-releases
naml update
```
