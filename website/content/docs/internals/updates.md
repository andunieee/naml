+++
title = "How updates work"
description = "How naml decides an app has an update, downloads it, swaps it in and rolls it back."
weight = 30

[extra]
lead = "Updating is two steps: checking every app for news, then applying the updates you accept."
+++

## Checking

For each app that isn't locked, naml asks the question that fits where the
app came from:

```mermaid
flowchart TD
  app(["an installed app"]) --> locked{"locked?"}
  locked -->|"yes"| skip(["skip it"])
  locked -->|"no"| nostr{"from the catalog?"}
  nostr -->|"yes"| ev{"is its newest announcement<br/>for a different file?"}
  ev -->|"yes"| pn(["update from the catalog"])
  ev -->|"no"| none(["up to date"])
  ev -->|"announcement gone"| zs
  nostr -->|"no"| zs{"AppImage with<br/>update information?"}
  zs -->|"yes"| zc{"does the .zsync file describe<br/>a different file?"}
  zc -->|"yes"| pz(["zsync delta update"])
  zc -->|"no"| none
  zs -->|"no"| gh{"from GitHub?"}
  gh -->|"yes"| rel{"does the newest release have a<br/>different asset shaped like ours?"}
  rel -->|"yes"| pg(["update from GitHub"])
  rel -->|"no"| none
  gh -->|"no"| url{"from a url?"}
  url -->|"yes"| val{"did ETag or Last-Modified change?"}
  val -->|"yes"| pu(["download it again"])
  val -->|"no"| none
  url -->|"no, a local file"| none
```

- **Catalog apps** are checked together, with one query to the catalog
  servers per publisher. An announcement only counts if it is at least as new as the one
  the app was installed from.
- **Update information** (`zsync|<url>` or
  `gh-releases-zsync|<owner>|<repo>|<tag>|<pattern>`) points at a `.zsync`
  file; for the GitHub form, naml finds the release by tag (`latest`,
  `latest-pre`, `latest-all` or a fixed tag) and the asset matching the
  pattern. The `.zsync` file holds the new file's sha1, compared with the
  installed one.
- **GitHub apps** find "the same asset" in a new release by its shape: the
  file name with every number blurred, so `App-1.2.0-x86_64.AppImage`
  matches `App-1.3.1-x86_64.AppImage`.
- **Urls** are checked with a `HEAD` request. When the server gives no
  validator, the file is downloaded again, and nothing changes if it has the
  same hash.

## zsync delta updates

A `.zsync` file describes the new version as a list of blocks, each with a
cheap rolling checksum and a strong one. naml slides over the installed
file, finds the blocks it already has, copies those locally, and downloads
only the missing ones with http range requests:

```mermaid
sequenceDiagram
  participant n as naml
  participant s as update server
  n->>s: GET App.AppImage.zsync
  s-->>n: block size, length, sha1, checksums of every block
  n->>n: scan the installed AppImage for blocks with those checksums
  n->>s: GET App.AppImage with Range: the missing blocks
  s-->>n: the missing bytes
  n->>n: assemble the new file, check its sha1
```

For a typical release only a small part of the file changes. If the delta
fails, or for catalog apps doesn't give the published sha256, naml falls back
to downloading the whole file.

## Applying

```mermaid
flowchart LR
  dl["download or<br/>delta-assemble"] --> stage["unpack into a<br/>scratch directory"]
  stage --> exe["pick the executable<br/>(yours, if still there)"]
  exe --> swap["current → previous/<br/>staged → apps/"]
  swap --> meta["refresh version,<br/>update information"]
  meta --> integ["rebuild the launcher,<br/>icon and command"]
  integ --> save[("save")]
```

The swap is two renames on the same filesystem: the installed directory
becomes the previous version (replacing the one kept before), and the staged
one takes its place. The new version's desktop entry and icon are read again,
and your [launcher edits](@/docs/guide/customizing.md#launchers) are applied
on top.

## Rolling back

```mermaid
flowchart LR
  subgraph before["before"]
    a1["apps/app = 2.0"]
    p1["previous/app = 1.9"]
  end
  subgraph after["after naml rollback"]
    a2["apps/app = 1.9"]
    p2["previous/app = 2.0"]
  end
  before --> after
```

A rollback exchanges the two directories (through a temporary name), along
with their versions, hashes and executables, and rebuilds the integration.
Running it again goes forward to the newer version. Locking the app keeps
`naml update` from undoing the rollback.
