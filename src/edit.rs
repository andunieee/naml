//! changing installed apps by hand: their name, id and executable, and their launcher.
//!
//! these changes are kept in the database, so updates, rollbacks and reinstalls redo them
//! instead of throwing them away.

use crate::Ctx;
use crate::db::{self, App};
use crate::desktop::Desktop;
use crate::format::{self, Format};
use crate::install;
use crate::ui;
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// redo the launcher, icon and command after a change, and save it
async fn reintegrate(ctx: &Ctx, app: &mut App) -> Result<()> {
    let scratch = ctx.paths.scratch()?;
    let bin = app.bin_link.is_some();
    install::integrate(ctx, app, bin, false, &scratch).await?;
    ctx.db.apps().update(app)?;
    Ok(())
}

/// the slug an app should have: its petname, or one from its "d" tag
fn slug_for(ctx: &Ctx, app: &App) -> Result<String> {
    install::choose_slug(
        ctx,
        &app.d,
        app.pubkey.as_deref(),
        app.petname.as_deref(),
        Some(&app.slug.0),
    )
}

/// move the app to the slug it should have, or just redo its integration if it has it already
async fn settle(ctx: &Ctx, app: &mut App) -> Result<()> {
    let slug = slug_for(ctx, app)?;
    if slug == app.slug.0 {
        reintegrate(ctx, app).await
    } else {
        install::relocate(ctx, app, slug).await
    }
}

pub async fn rename(ctx: &Ctx, app: &mut App, name: Option<String>) -> Result<()> {
    if let Some(name) = &name {
        install::validate_name(name)?;
        if ctx
            .db
            .apps()
            .by_petname(name.clone())?
            .is_some_and(|other| other.slug != app.slug)
            || (ctx.db.apps().contains(db::AppSlug::new(name.clone()))? && app.slug.0 != *name)
        {
            bail!("the name {name} is already taken");
        }
    }
    app.petname = name;
    settle(ctx, app).await
}

pub async fn set_id(ctx: &Ctx, app: &mut App, id: &str) -> Result<()> {
    if app.pubkey.is_some() {
        bail!(
            "{} comes from nostr, its id is the one its publisher gave it",
            app.handle()
        );
    }
    install::validate_name(id)?;
    // apps that don't say what they're called are named after their id
    if app.name == app.d {
        app.name = id.to_string();
    }
    app.d = id.to_string();
    settle(ctx, app).await
}

/// pick the executable by hand (a path inside the app directory, or just a file name), or go
/// back to letting naml pick it
pub async fn set_exec(ctx: &Ctx, app: &mut App, exec: Option<&str>) -> Result<()> {
    let fmt = Format::parse(&app.format).unwrap_or(Format::Binary);
    if matches!(fmt, Format::AppImage | Format::Binary) {
        bail!(
            "{} is a single {}, there is nothing else to run",
            app.handle(),
            fmt.as_str()
        );
    }
    let root = PathBuf::from(&app.root);
    let path = match exec {
        Some(exec) => find_in(&root, exec)?,
        None => {
            let hints = [app.slug.0.as_str(), app.d.as_str(), app.name.as_str()];
            format::find_executable(&root, None, &hints)?
        }
    };
    format::make_executable(&path)?;
    let rel = path.strip_prefix(&root)?.to_string_lossy().into_owned();
    app.exec_override = exec.map(|_| rel);
    app.exec = path.to_string_lossy().into_owned();
    reintegrate(ctx, app).await
}

/// a file of the app named by its path inside `root` (or an absolute path into it), or by its
/// file name when that is unique
fn find_in(root: &Path, exec: &str) -> Result<PathBuf> {
    let direct = if Path::new(exec).is_absolute() {
        PathBuf::from(exec)
    } else {
        root.join(exec)
    };
    if direct.starts_with(root) && !exec.split('/').any(|c| c == "..") && direct.is_file() {
        return Ok(direct);
    }
    let programs = format::programs(root);
    let named: Vec<&PathBuf> = programs
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == exec))
        .collect();
    if let [one] = named.as_slice() {
        return Ok((*one).clone());
    }
    let list = programs
        .iter()
        .filter_map(|p| p.strip_prefix(root).ok())
        .map(|p| format!("  {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");
    if named.is_empty() {
        bail!("there is no {exec} in the app, these could be run:\n{list}");
    }
    bail!("more than one file is called {exec}, give its path:\n{list}");
}

pub fn desktop_show(app: &App) -> Result<()> {
    let Some(path) = &app.desktop_file else {
        bail!(
            "{} isn't in the applications menu, `naml desktop create {}` puts it there",
            app.handle(),
            app.handle()
        );
    };
    let text = std::fs::read_to_string(path).with_context(|| format!("can't read {path}"))?;
    ui::step(path);
    print!("{text}");
    Ok(())
}

pub async fn desktop_create(ctx: &Ctx, app: &mut App) -> Result<()> {
    app.integrate = true;
    app.desktop_wanted = true;
    reintegrate(ctx, app).await
}

pub async fn desktop_remove(ctx: &Ctx, app: &mut App) -> Result<()> {
    app.integrate = false;
    app.desktop_wanted = false;
    reintegrate(ctx, app).await
}

fn validate_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.starts_with('-')
        || key.starts_with('[')
        || key.contains(['=', '\n', '#'])
        || key.trim() != key
    {
        bail!("{key:?} isn't a desktop entry key");
    }
    if key == "X-Naml-Slug" {
        bail!("{key} is how naml recognizes its launchers, it can't be changed");
    }
    Ok(())
}

/// replace whatever was changed by hand about `key`
fn put_override(app: &mut App, key: &str, change: Option<String>) {
    app.desktop_overrides.retain(|o| {
        let k = o
            .strip_prefix('-')
            .unwrap_or_else(|| o.split_once('=').map_or(o.as_str(), |(k, _)| k));
        k != key
    });
    if let Some(change) = change {
        app.desktop_overrides.push(change);
    }
}

/// set a key (`Some(Some(value))`), remove it (`Some(None)`), or forget the change to it
/// (`None`)
pub async fn desktop_set(
    ctx: &Ctx,
    app: &mut App,
    key: &str,
    value: Option<Option<&str>>,
) -> Result<()> {
    validate_key(key)?;
    let change = match value {
        Some(Some(v)) => {
            if v.contains('\n') {
                bail!("desktop entry values can't span lines");
            }
            Some(format!("{key}={v}"))
        }
        Some(None) => Some(format!("-{key}")),
        None => None,
    };
    put_override(app, key, change);
    if app.desktop_file.is_none() {
        app.integrate = true;
        app.desktop_wanted = true;
    }
    reintegrate(ctx, app).await
}

pub async fn desktop_reset(ctx: &Ctx, app: &mut App) -> Result<()> {
    app.desktop_overrides.clear();
    reintegrate(ctx, app).await
}

/// open the launcher in an editor and keep what was changed in its [Desktop Entry] group
pub async fn desktop_edit(ctx: &Ctx, app: &mut App) -> Result<()> {
    if app.desktop_file.is_none() {
        desktop_create(ctx, app).await?;
    }
    let Some(path) = app.desktop_file.clone() else {
        bail!("couldn't make a launcher for {}", app.handle());
    };
    let before = std::fs::read_to_string(&path).with_context(|| format!("can't read {path}"))?;

    let scratch = ctx.paths.scratch()?;
    let draft = scratch.join(format!("{}.desktop", app.slug.0));
    std::fs::write(&draft, &before)?;
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());
    // the editor may come with arguments, like "code --wait"
    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("sh")
        .arg(&draft)
        .status()
        .with_context(|| format!("failed to run {editor}"))?;
    if !status.success() {
        bail!("{editor} failed, nothing changed");
    }
    let after = std::fs::read_to_string(&draft)?;

    let old = first_values(&Desktop::parse(&before));
    let new = first_values(&Desktop::parse(&after));
    let mut changed = 0;
    for (key, value) in &new {
        if key != "X-Naml-Slug" && old.get(key) != Some(value) {
            put_override(app, key, Some(format!("{key}={value}")));
            changed += 1;
        }
    }
    for key in old.keys() {
        if key != "X-Naml-Slug" && !new.contains_key(key) {
            put_override(app, key, Some(format!("-{key}")));
            changed += 1;
        }
    }
    if changed == 0 {
        ui::step("nothing changed");
        return Ok(());
    }
    reintegrate(ctx, app).await?;
    ui::step(format!(
        "only the [Desktop Entry] group is kept, `naml desktop reset {}` undoes all changes",
        app.handle()
    ));
    Ok(())
}

fn first_values(desktop: &Desktop) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (k, v) in desktop.entries() {
        map.entry(k).or_insert(v);
    }
    map
}
