+++
title = "Updating"
description = "How naml finds and applies updates, rollbacks and automatic updates."
weight = 40

[extra]
lead = "`naml update` checks every app (or the ones you name), shows what changed, and updates them after asking."
+++

## Updating apps

```sh
naml update                # everything (aliases: naml up, naml upgrade)
naml update helix obsidian # just these
naml update --check        # only tell what can be updated
```

Locked apps are skipped. Each update is applied on its own: if one fails, the
others still go through.

## Where updates come from

How an app is updated depends on where it came from:

| source | how naml knows there's a new version | what it downloads |
|---|---|---|
| catalog | the publisher replaced its announcement with one pointing to a different file | that file, checked against its sha256; for AppImages with update information, a zsync delta when it gives the published hash |
| AppImage with update information | the `.zsync` file it points to describes a different file | only the changed blocks, with zsync |
| GitHub repository | a newer release has an asset shaped like the installed one | that asset |
| url | the server's `ETag`/`Last-Modified` changed | the file again; nothing changes if it is the same |

Update information is the AppImageUpdate kind embedded in an AppImage:
`zsync|<url>` and `gh-releases-zsync|<owner>|<repo>|<tag>|<pattern>`, with the
`latest`, `latest-pre` and `latest-all` tags.
[How updates work](@/docs/internals/updates.md) has the full decision tree.

What you set by hand survives updates: your
[chosen executable](@/docs/guide/customizing.md#choosing-the-executable)
(as long as the new version still has it), your
[launcher edits](@/docs/guide/customizing.md#launchers) and the app's name.

## Rolling back

Every update keeps the version it replaced. To go back to it:

```sh
naml rollback <app>    # or: naml downgrade
naml lock <app>        # optional: stay there
```

A rollback swaps the current and previous versions, so running it again
undoes it. `naml list` shows `rollback available` for apps that have a
previous version.

## Automatic updates

naml can update apps in the background with a systemd timer:

```sh
naml autoupdate enable
naml autoupdate status
naml autoupdate disable
```

`enable` writes `naml-update.service` and `naml-update.timer` to
`~/.config/systemd/user/` and starts the timer. It runs
`naml update --yes --quiet` ten minutes after boot and then every twelve
hours, with up to half an hour of random delay, and catches up on runs
missed while the machine was off.

With [`--system`](@/docs/guide/system-wide.md), the units go to
`/etc/systemd/system/` and update the apps installed for everyone as root.
