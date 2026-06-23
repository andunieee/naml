//! putting apps on disk, hooking them into the desktop and taking them out again

use crate::Ctx;
use crate::db::{App, AppSlug};
use crate::desktop::Desktop;
use crate::download::{self, Downloaded, Source};
use crate::format::{self, Format};
use crate::github;
use crate::nostr::Package;
use crate::{appimage, ui};
use anyhow::{Context, Result, bail};
use ritualistic::Event;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

pub struct Options {
    pub petname: Option<String>,
    /// identifier for apps installed from a url
    pub id: Option<String>,
    pub force: bool,
    pub desktop: bool,
    pub bin: bool,
}

/// everything we know about an app before downloading it
struct Origin {
    address: String,
    pubkey: Option<String>,
    d: String,
    name: Option<String>,
    summary: Option<String>,
    version: Option<String>,
    icon_url: Option<String>,
    urls: Vec<String>,
    expected_sha256: Option<String>,
    format: Option<Format>,
    exec: Option<String>,
    update_info: Option<String>,
    event: Option<String>,
    event_created_at: u64,
}

impl Origin {
    fn from_package(pkg: &Package, event: &Event) -> Self {
        Self {
            address: pkg.address(),
            pubkey: Some(pkg.pubkey.to_hex()),
            d: pkg.d.clone(),
            name: pkg.name.clone(),
            summary: pkg.summary.clone(),
            version: pkg.version.clone(),
            icon_url: pkg.icon.clone(),
            urls: pkg.urls.clone(),
            expected_sha256: Some(pkg.sha256.clone()),
            format: pkg.format.as_deref().and_then(Format::parse),
            exec: pkg.exec.clone(),
            update_info: pkg.update.clone(),
            event: serde_json::to_string(event).ok(),
            event_created_at: pkg.created_at,
        }
    }
}

pub async fn install(ctx: &Ctx, target: &str, opts: &Options) -> Result<App> {
    if let Some(repo) = github::Repo::parse(target) {
        let origin = github_origin(ctx, &repo, opts).await?;
        return install_origin(ctx, origin, opts).await;
    }
    let origin = match Source::parse(target) {
        Some(source) => {
            let (address, url, name) = match &source {
                Source::Http(url) => (
                    format!("url:{url}"),
                    url.clone(),
                    url.rsplit('/').next().unwrap_or("app").to_string(),
                ),
                Source::Local(path) => (
                    format!("file:{}", path.display()),
                    format!("file://{}", path.display()),
                    path.file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                ),
            };
            Origin {
                address,
                pubkey: None,
                d: opts
                    .id
                    .clone()
                    .unwrap_or_else(|| download::slug_from_filename(&name)),
                name: None,
                summary: None,
                version: None,
                icon_url: None,
                urls: vec![url],
                expected_sha256: None,
                format: None,
                exec: None,
                update_info: None,
                event: None,
                event_created_at: 0,
            }
        }
        None => {
            if opts.id.is_some() {
                bail!("--id only makes sense when installing from a url or a file");
            }
            let (pkg, event) = crate::resolve::listing(ctx, target).await?;
            if !pkg.supports_this_arch() {
                bail!(
                    "{} is only published for {}, this is {}",
                    pkg.d,
                    pkg.arch.join(", "),
                    std::env::consts::ARCH
                );
            }
            Origin::from_package(&pkg, &event)
        }
    };

    install_origin(ctx, origin, opts).await
}

/// the release asset that fits this machine, with what the repository says about itself
async fn github_origin(ctx: &Ctx, repo: &github::Repo, opts: &Options) -> Result<Origin> {
    let spinner = ui::spinner(format!("looking at {}/{}", repo.owner, repo.repo));
    let release = github::release_to_install(&ctx.http, repo).await;
    let info = github::repo_info(&ctx.http, repo).await.ok().flatten();
    spinner.finish_and_clear();
    let release = release?;

    let interactive = !ctx.yes && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let asset = github::pick(&release.assets, None, interactive)?;
    ui::step(format!(
        "{} from {}/{} {}",
        asset.name, repo.owner, repo.repo, release.tag_name
    ));

    Ok(Origin {
        address: repo.address(),
        pubkey: None,
        d: opts
            .id
            .clone()
            .unwrap_or_else(|| download::slug_from_filename(&repo.repo)),
        name: None,
        summary: info
            .and_then(|i| i.description)
            .filter(|d| !d.trim().is_empty()),
        version: Some(release.version()),
        icon_url: None,
        urls: vec![asset.browser_download_url.clone()],
        expected_sha256: None,
        format: None,
        exec: None,
        update_info: None,
        event: None,
        event_created_at: 0,
    })
}

async fn install_origin(ctx: &Ctx, origin: Origin, opts: &Options) -> Result<App> {
    if let Some(existing) = ctx.db.apps().by_address(origin.address.clone())? {
        if !opts.force {
            bail!(
                "{} is already installed as {}, use `nam update {}` (or --force to reinstall)",
                origin.d,
                existing.handle(),
                existing.handle()
            );
        }
        remove(ctx, &existing)?;
    }
    if let Some(petname) = &opts.petname {
        validate_name(petname)?;
        if ctx.db.apps().by_petname(petname.clone())?.is_some()
            || ctx.db.apps().contains(AppSlug::new(petname.clone()))?
        {
            bail!("the name {petname} is already taken");
        }
    }
    let slug = choose_slug(ctx, &origin, opts.petname.as_deref())?;

    let scratch = ctx.paths.scratch()?;
    let label = origin.name.clone().unwrap_or_else(|| origin.d.clone());
    let dl = download::fetch_any(
        &ctx.http,
        &origin.urls,
        origin.expected_sha256.as_deref(),
        &scratch,
        &label,
    )
    .await?;

    let format = match origin.format {
        Some(f) => f,
        None => Format::detect(&dl.path)?,
    };
    if matches!(format, Format::AppImage | Format::Binary)
        && let Some(arch) = format::elf_arch(&dl.path)
        && arch != std::env::consts::ARCH
    {
        bail!(
            "{} is built for {arch}, this is {}",
            origin.d,
            std::env::consts::ARCH
        );
    }
    let hints = [
        slug.as_str(),
        origin.d.as_str(),
        origin.name.as_deref().unwrap_or(""),
    ];
    let (staged, exec_rel) = stage(
        &dl.path,
        format,
        &scratch,
        &slug,
        origin.exec.as_deref(),
        &hints,
    )?;

    let root = ctx.paths.apps.join(&slug);
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::rename(&staged, &root)?;

    let now = ui::now();
    let mut app = App {
        slug: AppSlug::new(slug.clone()),
        petname: opts.petname.clone(),
        address: origin.address,
        pubkey: origin.pubkey,
        d: origin.d.clone(),
        name: origin.name.clone().unwrap_or_else(|| origin.d.clone()),
        summary: origin.summary,
        version: origin.version,
        format: format.as_str().to_string(),
        source_url: origin.urls[0].clone(),
        sha256: dl.sha256.clone(),
        size: dl.size,
        root: root.to_string_lossy().into_owned(),
        exec: root.join(&exec_rel).to_string_lossy().into_owned(),
        icon_url: origin.icon_url,
        desktop_file: None,
        icon_file: None,
        bin_link: None,
        update_info: origin.update_info,
        event: origin.event,
        event_created_at: origin.event_created_at,
        http_validator: dl.validator.clone(),
        previous_sha256: None,
        previous_version: None,
        previous_exec: None,
        locked: false,
        integrate: opts.desktop,
        installed_at: now,
        updated_at: now,
    };

    let result = async {
        integrate(ctx, &mut app, opts.bin, origin.name.is_none(), &scratch).await?;
        ctx.db.apps().insert(&app)?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Err(err) = result {
        // don't leave half-installed things around
        disintegrate(ctx, &app);
        let _ = std::fs::remove_dir_all(&root);
        return Err(err);
    }
    Ok(app)
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.starts_with('.')
        || name.contains('/')
        || name.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        bail!("{name:?} can't be used as a name");
    }
    Ok(())
}

fn choose_slug(ctx: &Ctx, origin: &Origin, petname: Option<&str>) -> Result<String> {
    if let Some(p) = petname {
        return Ok(p.to_string());
    }
    let base: String = origin
        .d
        .chars()
        .map(|c| {
            if c == '/' || c.is_whitespace() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let base = base.trim_start_matches('.').to_string();
    let base = if base.is_empty() {
        "app".to_string()
    } else {
        base
    };

    let free = |s: &str| -> Result<bool> {
        Ok(!ctx.db.apps().contains(AppSlug::new(s))?
            && ctx.db.apps().by_petname(s.to_string())?.is_none())
    };
    if free(&base)? {
        return Ok(base);
    }
    if let Some(pk) = &origin.pubkey {
        let npub = ui::npub(pk);
        let candidate = format!("{base}-{}", &npub[5..13]);
        if free(&candidate)? {
            return Ok(candidate);
        }
    }
    for i in 2.. {
        let candidate = format!("{base}-{i}");
        if free(&candidate)? {
            return Ok(candidate);
        }
    }
    unreachable!()
}

/// lay the app out in a directory under `scratch`, returns it and the executable's path in it
pub fn stage(
    file: &Path,
    format: Format,
    scratch: &Path,
    slug: &str,
    exec: Option<&str>,
    hints: &[&str],
) -> Result<(PathBuf, PathBuf)> {
    let dir = scratch.join("app");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    let rel = match format {
        // a fixed name, so the path stays the same across updates
        Format::AppImage => PathBuf::from(format!("{slug}.AppImage")),
        Format::Binary => PathBuf::from(slug),
        archive => {
            format::extract(file, archive, &dir)?;
            let exe = format::find_executable(&dir, exec, hints)?;
            let rel = exe.strip_prefix(&dir)?.to_path_buf();
            format::make_executable(&exe)?;
            return Ok((dir, rel));
        }
    };
    std::fs::rename(file, dir.join(&rel))?;
    format::make_executable(&dir.join(&rel))?;
    Ok((dir, rel))
}

/// create the launcher, icon and command for an app (replacing old ones)
pub async fn integrate(
    ctx: &Ctx,
    app: &mut App,
    bin: bool,
    take_name_from_desktop: bool,
    scratch: &Path,
) -> Result<()> {
    disintegrate(ctx, app);
    app.desktop_file = None;
    app.icon_file = None;
    app.bin_link = None;

    let exec = PathBuf::from(&app.exec);
    let root = PathBuf::from(&app.root);
    let format = Format::parse(&app.format).unwrap_or(Format::Binary);
    let slug = app.slug.0.clone();

    let mut desktop: Option<String> = None;
    let mut icon: Option<(Vec<u8>, &'static str)> = None;
    match format {
        Format::AppImage => {
            let info = appimage::elf_info(&exec)?;
            if app.update_info.is_none() {
                app.update_info = info.update_info;
            }
            let meta = match appimage::read_meta(&exec, info.payload_offset) {
                Ok(meta) => meta,
                Err(err) => {
                    ui::warn(format!("{err}, asking the AppImage itself"));
                    appimage::read_meta_with_runtime(&exec, scratch)?
                }
            };
            desktop = meta.desktop;
            icon = meta.icon;
        }
        Format::Binary => {}
        _ => {
            if let Some(path) = format::find_desktop(&root) {
                desktop = std::fs::read_to_string(path).ok();
                if let Some(name) = desktop
                    .as_deref()
                    .and_then(|d| Desktop::parse(d).get("Icon"))
                    && let Some(icon_path) = format::find_icon(&root, &name)
                    && let Ok(bytes) = std::fs::read(&icon_path)
                {
                    icon = Some((bytes.clone(), appimage::icon_ext(&bytes)));
                }
            }
        }
    }

    if let Some(text) = &desktop {
        let parsed = Desktop::parse(text);
        if take_name_from_desktop && let Some(name) = parsed.get("Name") {
            app.name = name;
        }
        if app.summary.is_none() {
            app.summary = parsed.get("Comment");
        }
        if app.version.is_none() {
            app.version = parsed.get("X-AppImage-Version");
        }
    }

    if icon.is_none()
        && let Some(url) = &app.icon_url
    {
        match fetch_icon(&ctx.http, url).await {
            Ok(bytes) => {
                let ext = appimage::icon_ext(&bytes);
                icon = Some((bytes, ext));
            }
            Err(err) => ui::warn(format!("couldn't get the icon: {err}")),
        }
    }

    if app.integrate {
        let icon_path = match &icon {
            Some((bytes, ext)) => {
                let path = ctx.paths.icons.join(format!("{slug}.{ext}"));
                std::fs::write(&path, bytes)?;
                app.icon_file = Some(path.to_string_lossy().into_owned());
                Some(path)
            }
            None => None,
        };
        let icon_str = icon_path
            .as_deref()
            .map(|p| p.to_string_lossy().into_owned());
        let exec_str = exec.to_string_lossy();

        // apps without a desktop file only get one when they look graphical: AppImages, or
        // whatever has an icon
        let entry = match &desktop {
            Some(text) => Some(Desktop::parse(text).rewrite(&exec_str, icon_str.as_deref(), &slug)),
            None if format == Format::AppImage || icon_str.is_some() => Some(Desktop::minimal(
                &app.name,
                app.summary.as_deref(),
                &exec_str,
                icon_str.as_deref(),
                &slug,
            )),
            None => None,
        };
        if let Some(entry) = entry {
            let path = ctx.paths.applications.join(format!("nam-{slug}.desktop"));
            std::fs::write(&path, entry)?;
            app.desktop_file = Some(path.to_string_lossy().into_owned());
            refresh_desktop_database(ctx);
        }
    }

    if bin {
        let link = ctx.paths.bin.join(app.handle());
        match std::fs::symlink_metadata(&link) {
            Ok(meta) if !(meta.file_type().is_symlink() && points_into(&link, &ctx.paths.apps)) => {
                ui::warn(format!(
                    "{} already exists and isn't ours, not linking the command",
                    link.display()
                ));
            }
            _ => {
                let _ = std::fs::remove_file(&link);
                std::os::unix::fs::symlink(&exec, &link)
                    .with_context(|| format!("failed to link {}", link.display()))?;
                app.bin_link = Some(link.to_string_lossy().into_owned());
            }
        }
    }

    Ok(())
}

fn points_into(link: &Path, dir: &Path) -> bool {
    std::fs::read_link(link).is_ok_and(|target| target.starts_with(dir))
}

async fn fetch_icon(http: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let response = http.get(url).send().await?;
    if !response.status().is_success() {
        bail!("{url} replied with {}", response.status());
    }
    let bytes = response.bytes().await?;
    if bytes.len() > 4 << 20 {
        bail!("icon is too big");
    }
    Ok(bytes.to_vec())
}

/// remove the launcher, icon and command of an app (but not the app)
pub fn disintegrate(ctx: &Ctx, app: &App) {
    let mut touched_desktop = false;
    if let Some(path) = &app.desktop_file {
        touched_desktop |= std::fs::remove_file(path).is_ok();
    }
    if let Some(path) = &app.icon_file {
        let _ = std::fs::remove_file(path);
    }
    if let Some(link) = &app.bin_link {
        let link = Path::new(link);
        if points_into(link, &ctx.paths.apps) {
            let _ = std::fs::remove_file(link);
        }
    }
    if touched_desktop {
        refresh_desktop_database(ctx);
    }
}

pub fn remove(ctx: &Ctx, app: &App) -> Result<()> {
    disintegrate(ctx, app);
    let root = Path::new(&app.root);
    if root.starts_with(&ctx.paths.apps) && root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    let previous = ctx.paths.previous.join(&app.slug.0);
    if previous.exists() {
        std::fs::remove_dir_all(previous)?;
    }
    ctx.db.apps().delete(app.slug.clone())?;
    Ok(())
}

/// swap in a new version staged at `staged`, keeping the current one as the previous version
pub fn replace_files(
    ctx: &Ctx,
    app: &mut App,
    staged: &Path,
    exec_rel: &Path,
    dl: &Downloaded,
) -> Result<()> {
    let root = PathBuf::from(&app.root);
    let previous = ctx.paths.previous.join(&app.slug.0);
    if previous.exists() {
        std::fs::remove_dir_all(&previous)?;
    }
    let old_exec_rel = Path::new(&app.exec)
        .strip_prefix(&root)
        .map(|p| p.to_string_lossy().into_owned())
        .ok();
    if root.exists() {
        std::fs::rename(&root, &previous)?;
    }
    std::fs::rename(staged, &root)?;

    app.previous_sha256 = Some(std::mem::replace(&mut app.sha256, dl.sha256.clone()));
    app.previous_version = app.version.take();
    app.previous_exec = old_exec_rel;
    app.exec = root.join(exec_rel).to_string_lossy().into_owned();
    app.size = dl.size;
    app.updated_at = ui::now();
    Ok(())
}

pub async fn rollback(ctx: &Ctx, app: &mut App) -> Result<()> {
    let root = PathBuf::from(&app.root);
    let previous = ctx.paths.previous.join(&app.slug.0);
    let (Some(prev_sha), Some(prev_exec)) =
        (app.previous_sha256.clone(), app.previous_exec.clone())
    else {
        bail!(
            "there is no previous version of {} to go back to",
            app.handle()
        );
    };
    if !previous.exists() {
        bail!("the previous version of {} is gone", app.handle());
    }

    let swap = ctx.paths.previous.join(format!(".{}-swap", app.slug.0));
    let current_exec = Path::new(&app.exec)
        .strip_prefix(&root)
        .map(|p| p.to_string_lossy().into_owned())
        .ok();
    std::fs::rename(&root, &swap)?;
    std::fs::rename(&previous, &root)?;
    std::fs::rename(&swap, &previous)?;

    let current_version = app.version.take();
    app.version = app.previous_version.take();
    app.previous_version = current_version;
    app.previous_sha256 = Some(std::mem::replace(&mut app.sha256, prev_sha));
    app.previous_exec = current_exec;
    app.exec = root.join(prev_exec).to_string_lossy().into_owned();
    app.updated_at = ui::now();

    let scratch = ctx.paths.scratch()?;
    let bin = app.bin_link.is_some();
    integrate(ctx, app, bin, false, &scratch).await?;
    ctx.db.apps().update(app)?;
    Ok(())
}

fn refresh_desktop_database(ctx: &Ctx) {
    let _ = std::process::Command::new("update-desktop-database")
        .arg("-q")
        .arg(&ctx.paths.applications)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}
