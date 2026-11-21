+++
title = "The catalog"
description = "Finding apps, your identity, recommendations and catalog servers."
weight = 70

[extra]
lead = "naml's catalog lives on nostr: publishers sign announcements of their apps, and you see what the people you follow recommend. There is no central server."
+++

## Searching

```sh
naml search editor        # or: naml s
naml search -r            # only apps recommended by people you follow
naml info helix           # details of an app that isn't installed
```

Results are ranked by how well they match the id, the name, the tags, the
summary and the description, then by how many people you follow recommend
them (`★3`). Ids used by more than one publisher are shown qualified, as
`<npub>/<id>`. Installed apps are marked `[installed]`.

## Syncing

```sh
naml sync
```

`sync` refreshes the local copy of the catalog: every app announcement the
catalog servers have, plus the specific apps recommended by the people you
follow, found through your follow list and their recommendation lists.
naml does this by itself when the catalog is more than a day old and you
search or install by id. [The catalog protocol](@/docs/internals/catalog.md)
describes the events involved.

## Your identity

You don't need one to install apps. With one, naml knows who you follow, so
it can show their recommendations, and you can publish.

```sh
naml login npub1…                 # read-only: recommendations from your follows
naml login you@example.com        # read-only, through NIP-05
naml login bunker://…             # a remote signer (NIP-46): can publish
naml login nsec1…                 # a secret key: can publish
naml whoami
naml logout
```

A secret key given to `login` is stored unencrypted in naml's database
(`~/.local/share/naml/naml.redb`). A remote signer (`bunker://`) keeps the
key out of naml altogether. Approve the connection on the signer when asked.

## Recommending apps

The catalog apps you have installed are recommended automatically. Adjust the
list and publish it, so the people who follow you see it:

```sh
naml recommend list
naml recommend add <app>       # installed or not
naml recommend remove <app>    # even if it is installed
naml recommend publish
```

Publishing keeps what you recommended from other machines, unless you
removed it here.

## Catalog servers

The catalog servers are nostr relays. naml always uses
`wss://relay.damus.io`, `wss://nos.lol`, `wss://relay.primal.net` and
`wss://nostr.mom`. When syncing, it also asks the eight relays most listed
for naml by the people you follow. Add your own:

```sh
naml servers list
naml servers add wss://relay.example.com
naml servers remove wss://relay.example.com
naml servers publish     # tell your followers which ones you use
```
