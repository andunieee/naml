//! moving an app between the user's apps and the ones installed for everyone.
//!
//! each side's database is only opened by its owner: the user's naml does its side and asks
//! naml running as root (through sudo or doas) for the system side, handing the app over as
//! json. that way nothing root-owned ends up in the user's home.

use crate::Ctx;
use crate::db::App;
use crate::install;
use crate::ui;
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// an app on its way to the other side, with where its files are now
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Transfer {
    pub app: App,
    pub root: PathBuf,
    pub previous: Option<PathBuf>,
}

impl Transfer {
    pub fn of(ctx: &Ctx, app: App) -> Self {
        let previous = ctx.paths.previous.join(&app.slug.0);
        Self {
            root: PathBuf::from(&app.root),
            previous: previous.exists().then_some(previous),
            app,
        }
    }
}

pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions and can't fail
    unsafe { libc::geteuid() == 0 }
}

/// run naml as root with `args`, feeding it `input`; its stdout is returned, its stderr is
/// shown as it goes
fn as_root(args: &[&str], input: Option<&[u8]>) -> Result<Output> {
    let exe = std::env::current_exe().context("can't find naml itself")?;
    let mut helpers: Vec<Command> = vec![];
    if is_root() {
        helpers.push(Command::new(&exe));
    } else {
        for helper in ["sudo", "doas"] {
            let mut cmd = Command::new(helper);
            cmd.arg(&exe);
            helpers.push(cmd);
        }
    }
    for mut cmd in helpers {
        cmd.args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::inherit()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err).context("failed to ask for root"),
        };
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin.write_all(input)?;
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!("naml as root failed");
        }
        return Ok(output);
    }
    bail!("apps for everyone are managed as root, and neither sudo nor doas is here")
}

fn check_not_sudo() -> Result<()> {
    if is_root() && std::env::var_os("SUDO_USER").is_some() {
        bail!("run `naml move` as yourself, it asks for root when it needs it");
    }
    Ok(())
}

/// move one of the user's apps to the ones installed for everyone
pub fn to_system(ctx: &Ctx, app: &App) -> Result<()> {
    check_not_sudo()?;
    let transfer = serde_json::to_vec(&Transfer::of(ctx, app.clone()))?;
    as_root(&["--system", "__receive"], Some(&transfer))?;
    install::remove(ctx, app)?;
    Ok(())
}

/// move an app installed for everyone to the user's apps
pub async fn to_user(ctx: &Ctx, input: &str) -> Result<App> {
    check_not_sudo()?;
    let output = as_root(&["--system", "__send", input], None)?;
    let transfer: Transfer =
        serde_json::from_slice(&output.stdout).context("naml as root sent something odd")?;
    let system_slug = transfer.app.slug.0.clone();
    let app = adopt(
        ctx,
        transfer.app,
        &transfer.root,
        transfer.previous.as_deref(),
    )
    .await?;
    if let Err(err) = as_root(
        &["--system", "--yes", "--quiet", "remove", &system_slug],
        None,
    ) {
        ui::warn(format!(
            "{} is yours now, but the copy for everyone is still there ({err}), \
             `naml --system remove {system_slug}` removes it",
            app.handle()
        ));
    }
    Ok(app)
}

/// the root side of [`to_system`]: take an app handed over on stdin
pub async fn receive(ctx: &Ctx) -> Result<()> {
    if !ctx.paths.system {
        bail!("only for naml --system");
    }
    let transfer: Transfer = serde_json::from_reader(std::io::stdin().lock())?;
    // whoever asked through sudo can only hand over their own files
    if let Some(uid) = std::env::var("SUDO_UID")
        .ok()
        .and_then(|u| u.parse::<u32>().ok())
    {
        let owner = std::fs::symlink_metadata(&transfer.root)
            .with_context(|| format!("can't read {}", transfer.root.display()))?
            .uid();
        if owner != uid {
            bail!("{} isn't yours", transfer.root.display());
        }
    }
    adopt(
        ctx,
        transfer.app,
        &transfer.root,
        transfer.previous.as_deref(),
    )
    .await?;
    Ok(())
}

/// the root side of [`to_user`]: print an app for the user's naml to take
pub async fn send(ctx: &Ctx, input: &str) -> Result<()> {
    if !ctx.paths.system {
        bail!("only for naml --system");
    }
    let app = crate::resolve::installed(ctx, input).await?;
    println!("{}", serde_json::to_string(&Transfer::of(ctx, app))?);
    Ok(())
}

/// install an app from another naml's files: copy them in, link it up and keep its record,
/// with everything that was set by hand
async fn adopt(ctx: &Ctx, mut app: App, from: &Path, previous: Option<&Path>) -> Result<App> {
    if let Some(existing) = ctx.db.apps().by_address(app.address.clone())? {
        bail!(
            "{} is already installed {} as {}",
            app.d,
            if ctx.paths.system {
                "for everyone"
            } else {
                "for you"
            },
            existing.handle()
        );
    }
    if let Some(petname) = &app.petname
        && (ctx.db.apps().by_petname(petname.clone())?.is_some()
            || ctx
                .db
                .apps()
                .contains(crate::db::AppSlug::new(petname.clone()))?)
    {
        bail!("the name {petname} is already taken there, rename the app first");
    }
    let slug = install::choose_slug(
        ctx,
        &app.d,
        app.pubkey.as_deref(),
        app.petname.as_deref(),
        None,
    )?;

    let share = ctx.paths.system;
    let scratch = ctx.paths.scratch()?;
    let root = ctx.paths.apps.join(&slug);
    let prev = ctx.paths.previous.join(&slug);
    if root.exists() {
        bail!("{} is in the way", root.display());
    }
    ui::step(format!("copying {}", from.display()));
    copy_tree(from, &scratch.join("app"), share)?;
    std::fs::rename(scratch.join("app"), &root)?;
    match previous {
        Some(p) if p.exists() => {
            if prev.exists() {
                std::fs::remove_dir_all(&prev)?;
            }
            copy_tree(p, &scratch.join("previous"), share)?;
            std::fs::rename(scratch.join("previous"), &prev)?;
        }
        _ => {
            app.previous_sha256 = None;
            app.previous_version = None;
            app.previous_exec = None;
        }
    }

    let exec_rel = Path::new(&app.exec)
        .strip_prefix(from)
        .with_context(|| format!("{} isn't inside {}", app.exec, from.display()))?
        .to_path_buf();
    app.exec = root.join(exec_rel).to_string_lossy().into_owned();
    app.root = root.to_string_lossy().into_owned();
    app.slug = crate::db::AppSlug::new(slug);
    app.desktop_file = None;
    app.icon_file = None;
    let bin = app.bin_link.take().is_some();

    let result = async {
        install::integrate(ctx, &mut app, bin, false, &scratch).await?;
        ctx.db.apps().insert(&app)?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(err) = result {
        install::disintegrate(ctx, &app);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&prev);
        return Err(err);
    }
    Ok(app)
}

/// copy a directory tree, keeping modes and symlinks; with `share`, whatever its owner could
/// read or run, everyone can
fn copy_tree(src: &Path, dst: &Path, share: bool) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src).with_context(|| format!("can't read {}", src.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            std::os::unix::fs::symlink(std::fs::read_link(&from)?, &to)?;
            continue;
        }
        if ft.is_dir() {
            copy_tree(&from, &to, share)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to)
                .with_context(|| format!("failed to copy {}", from.display()))?;
        } else {
            continue;
        }
        if share {
            let mut perms = std::fs::metadata(&to)?.permissions();
            let owner = perms.mode() & 0o500;
            perms.set_mode(perms.mode() | owner >> 3 | owner >> 6);
            std::fs::set_permissions(&to, perms)?;
        }
    }
    if share {
        let mut perms = std::fs::metadata(dst)?.permissions();
        perms.set_mode(perms.mode() | 0o755);
        std::fs::set_permissions(dst, perms)?;
    }
    Ok(())
}
