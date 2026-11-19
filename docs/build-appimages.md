# building AppImages that naml can install and update

This guide is written for coding agents. It covers producing an AppImage with a
proper desktop entry and embedded update information, so that naml (and any
other AppImage tool) can add it to the applications menu and update it with
small delta downloads. Publishing the result is covered in
[publish-from-github-actions.md](publish-from-github-actions.md).

## 1. lay out the AppDir

```
MyApp.AppDir/
  AppRun                  # executable entry point (or a symlink to the binary)
  myapp.desktop           # desktop entry, at the root
  myapp.png               # icon named like the Icon= key, at the root
  .DirIcon -> myapp.png   # symlink, what file managers show
  usr/bin/myapp           # the app and its libraries
```

`AppRun` can be a shell script: `exec "$APPDIR/usr/bin/myapp" "$@"`. For apps
that need bundled libraries, `linuxdeploy` can assemble the AppDir for you
(`linuxdeploy --appdir MyApp.AppDir -e myapp -d myapp.desktop -i myapp.png`).

## 2. write the desktop entry

```ini
[Desktop Entry]
Type=Application
Name=My App
Comment=One line saying what it does
Exec=myapp %U
Icon=myapp
Categories=Utility;
X-AppImage-Version=1.2.3
```

naml reads these keys straight from the AppImage, without running it, and uses
them when nothing else is given:

- `Name` and `Comment` become the app's name and summary.
- `X-AppImage-Version` becomes its version. Set it from the release tag in CI.
- `Categories` become its tags.
- `Icon` must match the icon file name without extension (`myapp.png`).

When installing, naml rewrites `Exec=` to the installed AppImage and `Icon=` to
the extracted icon, so keep `Exec=` as the plain command name. Use a PNG
(256x256 or larger) or an SVG for the icon.

## 3. embed update information and build

Use `appimagetool` with `-u`. For GitHub releases:

```sh
ARCH=x86_64 appimagetool \
  -u "gh-releases-zsync|<owner>|<repo>|latest|MyApp-*x86_64.AppImage.zsync" \
  MyApp.AppDir MyApp-x86_64.AppImage
```

This embeds the update information in the AppImage and writes
`MyApp-x86_64.AppImage.zsync` next to it. The fields are:

| field | meaning |
| --- | --- |
| `gh-releases-zsync` | look up a GitHub release and fetch the matching `.zsync` asset |
| owner, repo | the GitHub repository |
| tag | `latest` (newest stable), `latest-pre` (newest prerelease), `latest-all` (either), or a fixed tag |
| pattern | glob matched against the release's asset names; must match the `.zsync` file and, with `*`, the version part |

For any other host use `zsync|https://example.com/MyApp-x86_64.AppImage.zsync`.
Other update types (bintray, pling) are not supported by naml.

The `.zsync` file contains the download URL of the AppImage, so upload both
files to the same place. Rebuilding the AppImage means regenerating the
`.zsync`; they must always be a matching pair.

## 4. in GitHub Actions

```yaml
- name: build AppImage
  run: |
    curl -fsSLo appimagetool \
      https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x appimagetool
    sed -i "s/^X-AppImage-Version=.*/X-AppImage-Version=${GITHUB_REF_NAME#v}/" MyApp.AppDir/myapp.desktop
    ARCH=x86_64 ./appimagetool --appimage-extract-and-run \
      -u "gh-releases-zsync|${GITHUB_REPOSITORY%/*}|${GITHUB_REPOSITORY#*/}|latest|MyApp-*x86_64.AppImage.zsync" \
      MyApp.AppDir MyApp-x86_64.AppImage

- uses: softprops/action-gh-release@v2
  with:
    files: |
      MyApp-x86_64.AppImage
      MyApp-x86_64.AppImage.zsync
```

`--appimage-extract-and-run` is needed because runners have no FUSE. For an
aarch64 build use an `ubuntu-24.04-arm` runner, `ARCH=aarch64` and the
`appimagetool-aarch64.AppImage` download.

## checklist

- [ ] The `.desktop` file and the icon are at the AppDir root and `Icon=` matches the icon's name.
- [ ] `X-AppImage-Version` is set from the release, not hardcoded.
- [ ] Both the `.AppImage` and the `.zsync` are uploaded to the release.
- [ ] The update pattern matches the real `.zsync` asset name (check it with `naml publish --dry-run`, which prints the update information it found).
- [ ] The AppImage runs: `chmod +x MyApp-x86_64.AppImage && ./MyApp-x86_64.AppImage`.
- [ ] `naml install ./MyApp-x86_64.AppImage` puts it in the applications menu.
