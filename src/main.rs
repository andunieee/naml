mod appimage;
mod db;
mod desktop;
mod download;
mod format;
mod github;
mod install;
mod nostr;
mod paths;
mod resolve;
mod social;
mod ui;
mod update;
mod zsync;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use console::style;

pub struct Ctx {
    pub paths: paths::Paths,
    pub db: db::Db,
    pub http: reqwest::Client,
    pub net: ritualistic::Network,
    pub yes: bool,
}

#[derive(Parser)]
#[command(
    name = "nam",
    version,
    about = "install, update and integrate portable linux apps found over nostr",
    after_help = "apps can be referred to by name, by \"d\" tag, or as <npub|nip05|hex>/<d> when the \"d\" tag is ambiguous"
)]
struct Cli {
    /// answer yes to every question
    #[arg(short, long, global = true)]
    yes: bool,
    /// only print errors
    #[arg(short, long, global = true)]
    quiet: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// install apps from the catalog, a url or a local file
    #[command(visible_alias = "i")]
    Install {
        /// "d" tag, <author>/<d>, naddr, github repository, https:// url or path to a file
        #[arg(required = true)]
        targets: Vec<String>,
        /// a name of your own for the app
        #[arg(short, long)]
        name: Option<String>,
        /// identifier for an app installed from a url or file (defaults to one from the file name)
        #[arg(long)]
        id: Option<String>,
        /// reinstall if already installed
        #[arg(short, long)]
        force: bool,
        /// don't add it to the applications menu
        #[arg(long)]
        no_desktop: bool,
        /// don't link a command in ~/.local/bin
        #[arg(long)]
        no_bin: bool,
    },
    /// uninstall apps
    #[command(visible_aliases = ["rm", "uninstall"])]
    Remove {
        #[arg(required = true)]
        apps: Vec<String>,
    },
    /// list installed apps
    #[command(visible_alias = "ls")]
    List,
    /// details of an installed or available app
    Info { app: String },
    /// look for apps in the catalog
    #[command(visible_alias = "s")]
    Search {
        query: Vec<String>,
        /// only apps recommended by people you follow
        #[arg(short, long)]
        recommended: bool,
    },
    /// refresh the catalog from relays and recommendations
    Sync,
    /// update apps (all of them by default)
    #[command(visible_aliases = ["up", "upgrade"])]
    Update {
        apps: Vec<String>,
        /// only tell what can be updated
        #[arg(short, long)]
        check: bool,
    },
    /// go back to the version before the last update
    #[command(visible_alias = "downgrade")]
    Rollback { app: String },
    /// keep an app at its current version
    Lock { app: String },
    /// let an app update again
    Unlock { app: String },
    /// run an app
    #[command(visible_alias = "x")]
    Run {
        app: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// give an app a name of your own, or drop it
    #[command(visible_alias = "rename")]
    Name { app: String, name: Option<String> },
    /// set your identity: nsec, bunker:// (to publish) or npub/nip-05 (read-only)
    Login { key: String },
    /// forget your identity
    Logout,
    /// show your identity
    Whoami,
    /// manage and publish the apps you recommend
    #[command(subcommand)]
    Recommend(RecommendCommand),
    /// manage and publish the relays you use for apps
    #[command(subcommand)]
    Relays(RelaysCommand),
    /// announce an app on nostr
    Publish(Box<PublishCommand>),
    /// update apps in the background with a systemd user timer
    Autoupdate {
        #[arg(value_parser = ["enable", "disable", "status"])]
        action: String,
    },
}

#[derive(Subcommand)]
enum RecommendCommand {
    /// show what you recommend (installed apps plus manual edits)
    List,
    /// recommend an app, installed or not
    Add { app: String },
    /// stop recommending an app
    Remove { app: String },
    /// publish your recommendations
    Publish,
}

#[derive(Subcommand)]
enum RelaysCommand {
    List,
    Add {
        url: String,
    },
    Remove {
        url: String,
    },
    /// publish your relays as a NIP-51 relay set
    Publish,
}

#[derive(clap::Args)]
struct PublishCommand {
    /// urls the app can be downloaded from, mirrors after the first
    #[arg(required = true)]
    urls: Vec<String>,
    /// hash this local file instead of downloading the first url
    #[arg(long)]
    file: Option<String>,
    /// the "d" tag (defaults to one from the file name)
    #[arg(long)]
    id: Option<String>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    summary: Option<String>,
    #[arg(long = "app-version")]
    version: Option<String>,
    /// url of an icon
    #[arg(long)]
    icon: Option<String>,
    /// homepage
    #[arg(long)]
    web: Option<String>,
    #[arg(long)]
    license: Option<String>,
    /// appimage, binary, tar.gz, tar.xz, tar.zst, tar.bz2, tar or zip (detected by default)
    #[arg(long)]
    format: Option<String>,
    /// for archives, the path of the executable inside it
    #[arg(long)]
    exec: Option<String>,
    /// AppImageUpdate update information (read from the AppImage by default)
    #[arg(long)]
    update: Option<String>,
    /// supported architectures (detected from the file by default)
    #[arg(long)]
    arch: Vec<String>,
    /// categories or keywords
    #[arg(short, long = "tag")]
    tags: Vec<String>,
    #[arg(long)]
    description: Option<String>,
    /// print the event without publishing
    #[arg(long)]
    dry_run: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    ui::set_quiet(cli.quiet);
    if let Err(err) = run(cli).await {
        eprintln!("{} {err:#}", style("error:").red().bold());
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> Result<()> {
    let paths = paths::Paths::new()?;
    let db = db::Db::open(&paths.db).with_context(|| {
        format!(
            "can't open {} (is another nam running?)",
            paths.db.display()
        )
    })?;
    let ctx = Ctx {
        paths,
        db,
        http: download::client(),
        net: ritualistic::Network::new(),
        yes: cli.yes,
    };

    match cli.command {
        Command::Install {
            targets,
            name,
            id,
            force,
            no_desktop,
            no_bin,
        } => {
            if targets.len() > 1 && (name.is_some() || id.is_some()) {
                bail!("--name and --id only work when installing one app");
            }
            let opts = install::Options {
                petname: name,
                id,
                force,
                desktop: !no_desktop,
                bin: !no_bin,
            };
            let mut failed = 0;
            for target in &targets {
                match install::install(&ctx, target, &opts).await {
                    Ok(app) => ui::success(format!(
                        "installed {} {}{}",
                        style(app.handle()).bold(),
                        app.version.as_deref().unwrap_or(""),
                        match &app.bin_link {
                            Some(_) => format!(" (run it with `{}`)", app.handle()),
                            None => String::new(),
                        }
                    )),
                    Err(err) => {
                        failed += 1;
                        eprintln!("{} {target}: {err:#}", style("error:").red().bold());
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} of {} installs failed", targets.len());
            }
        }

        Command::Remove { apps } => {
            for input in apps {
                let app = resolve::installed(&ctx, &input).await?;
                if !ui::confirm(&format!("remove {}?", app.handle()), ctx.yes) {
                    continue;
                }
                install::remove(&ctx, &app)?;
                ui::success(format!("removed {}", app.handle()));
            }
        }

        Command::List => {
            let apps = ctx.db.apps().all()?;
            if apps.is_empty() {
                ui::step("nothing installed yet, try `nam search`");
            }
            for app in apps {
                let flags = [
                    app.locked.then_some("locked"),
                    app.previous_sha256.as_ref().map(|_| "rollback available"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ");
                println!(
                    "{:<20} {:<28} {:<14} {:<9} {}{}",
                    style(app.handle()).bold(),
                    truncate(&app.name, 28),
                    truncate(app.version.as_deref().unwrap_or("-"), 14),
                    app.format,
                    style(source_of(&app)).dim(),
                    if flags.is_empty() {
                        String::new()
                    } else {
                        format!(" [{flags}]")
                    }
                );
            }
        }

        Command::Info { app } => info(&ctx, &app).await?,

        Command::Search { query, recommended } => {
            nostr::ensure_fresh(&ctx).await?;
            search(&ctx, &query.join(" ").to_lowercase(), recommended)?;
        }

        Command::Sync => {
            let report = nostr::sync(&ctx).await?;
            ui::success(format!(
                "{} apps from {} relays, {} recommended by the {} people you follow",
                report.apps, report.relays, report.recommended, report.follows
            ));
        }

        Command::Update { apps, check } => {
            let targets = if apps.is_empty() {
                ctx.db.apps().all()?
            } else {
                let mut v = vec![];
                for a in apps {
                    v.push(resolve::installed(&ctx, &a).await?);
                }
                v
            };
            let result = update::check(&ctx, targets).await?;
            for (app, err) in &result.failed {
                ui::warn(format!("{}: {err:#}", app.handle()));
            }
            if result.available.is_empty() {
                ui::success("everything is up to date");
                return Ok(());
            }
            for available in &result.available {
                println!("  {}", available.describe());
            }
            if check {
                return Ok(());
            }
            if !ui::confirm(&format!("update {} apps?", result.available.len()), ctx.yes) {
                return Ok(());
            }
            let mut failed = 0;
            for available in result.available {
                let handle = available.app.handle().to_string();
                ui::step(format!("updating {handle}"));
                match update::apply(&ctx, available).await {
                    Ok(true) => ui::success(format!("updated {handle}")),
                    Ok(false) => ui::success(format!("{handle} didn't actually change")),
                    Err(err) => {
                        failed += 1;
                        eprintln!("{} {handle}: {err:#}", style("error:").red().bold());
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} updates failed");
            }
        }

        Command::Rollback { app } => {
            let mut app = resolve::installed(&ctx, &app).await?;
            install::rollback(&ctx, &mut app).await?;
            ui::success(format!(
                "{} is back at {} (`nam lock {}` keeps it there)",
                app.handle(),
                app.version.as_deref().unwrap_or("the previous version"),
                app.handle()
            ));
        }

        Command::Lock { app } => set_locked(&ctx, &app, true).await?,
        Command::Unlock { app } => set_locked(&ctx, &app, false).await?,

        Command::Run { app, args } => {
            use std::os::unix::process::CommandExt;
            let app = resolve::installed(&ctx, &app).await?;
            let err = std::process::Command::new(&app.exec).args(args).exec();
            bail!("failed to run {}: {err}", app.exec);
        }

        Command::Name { app, name } => {
            let mut app = resolve::installed(&ctx, &app).await?;
            if let Some(name) = &name
                && (ctx.db.apps().by_petname(name.clone())?.is_some()
                    || (ctx.db.apps().contains(db::AppSlug::new(name.clone()))?
                        && app.slug.0 != *name))
            {
                bail!("the name {name} is already taken");
            }
            app.petname = name;
            let scratch = ctx.paths.scratch()?;
            let bin = app.bin_link.is_some();
            install::integrate(&ctx, &mut app, bin, false, &scratch).await?;
            ctx.db.apps().update(&app)?;
            ui::success(format!(
                "{} is now called {}",
                app.qualified(),
                app.handle()
            ));
        }

        Command::Login { key } => {
            let pk = nostr::login(&ctx, &key).await?;
            let (_, how) = social::whoami(&ctx)?.unwrap_or_default();
            ui::success(format!("logged in as {} ({how})", pk.to_npub()));
            ui::step("your follows' recommendations will show up after `nam sync`");
        }

        Command::Logout => {
            for key in ["secret", "bunker", "bunker_client", "pubkey"] {
                db::unset_setting(&ctx.db, key)?;
            }
            ui::success("logged out");
        }

        Command::Whoami => match social::whoami(&ctx)? {
            Some((npub, how)) => println!("{npub} ({how})"),
            None => println!("not logged in"),
        },

        Command::Recommend(cmd) => match cmd {
            RecommendCommand::List => {
                for addr in social::recommended(&ctx)? {
                    let (pk, d) = addr.split_once(':').unwrap_or(("", &addr));
                    println!("{}/{}", ui::npub(pk), d);
                }
            }
            RecommendCommand::Add { app } => {
                let address = match resolve::installed(&ctx, &app).await {
                    Ok(installed) if installed.pubkey.is_some() => installed.address,
                    _ => resolve::listing(&ctx, &app).await?.0.address(),
                };
                social::set_recommendation(&ctx, &address, true)?;
                ui::success("added, `nam recommend publish` to share it");
            }
            RecommendCommand::Remove { app } => {
                let address = match resolve::installed(&ctx, &app).await {
                    Ok(installed) => installed.address,
                    Err(_) => resolve::listing(&ctx, &app).await?.0.address(),
                };
                social::set_recommendation(&ctx, &address, false)?;
                ui::success("removed, `nam recommend publish` to share it");
            }
            RecommendCommand::Publish => {
                let n = social::publish_recommendations(&ctx).await?;
                ui::success(format!("published {n} recommendations"));
            }
        },

        Command::Relays(cmd) => match cmd {
            RelaysCommand::List => {
                for relay in nostr::DEFAULT_RELAYS {
                    println!("{relay} {}", style("(default)").dim());
                }
                for relay in ctx.db.user_relays().all()? {
                    println!("{}", relay.url.0);
                }
            }
            RelaysCommand::Add { url } => {
                let url = social::add_relay(&ctx, &url)?;
                ui::success(format!("added {url}"));
            }
            RelaysCommand::Remove { url } => {
                if !social::remove_relay(&ctx, &url)? {
                    bail!("{url} isn't one of your relays");
                }
                ui::success(format!("removed {url}"));
            }
            RelaysCommand::Publish => {
                let n = social::publish_relays(&ctx).await?;
                ui::success(format!("published {n} relays"));
            }
        },

        Command::Publish(p) => {
            social::publish_app(
                &ctx,
                social::PublishArgs {
                    urls: p.urls,
                    file: p.file,
                    id: p.id,
                    name: p.name,
                    summary: p.summary,
                    version: p.version,
                    icon: p.icon,
                    web: p.web,
                    license: p.license,
                    format: p.format,
                    exec: p.exec,
                    update: p.update,
                    arch: p.arch,
                    topics: p.tags,
                    description: p.description,
                    dry_run: p.dry_run,
                },
            )
            .await?
        }

        Command::Autoupdate { action } => autoupdate(&ctx, &action)?,
    }

    Ok(())
}

async fn set_locked(ctx: &Ctx, input: &str, locked: bool) -> Result<()> {
    let mut app = resolve::installed(ctx, input).await?;
    app.locked = locked;
    ctx.db.apps().update(&app)?;
    ui::success(format!(
        "{} {}",
        app.handle(),
        if locked {
            "won't be updated"
        } else {
            "will be updated again"
        }
    ));
    Ok(())
}

fn source_of(app: &db::App) -> String {
    match &app.pubkey {
        Some(_) => app.qualified(),
        None => app.source_url.clone(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

async fn info(ctx: &Ctx, input: &str) -> Result<()> {
    let row = |k: &str, v: &str| println!("{:>14}  {v}", style(k).dim());

    if let Ok(app) = resolve::installed(ctx, input).await {
        println!("{}", style(&app.name).bold());
        if let Some(s) = &app.summary {
            println!("{s}");
        }
        println!();
        row("name", app.handle());
        row("id", &app.qualified());
        row("version", app.version.as_deref().unwrap_or("-"));
        row("format", &app.format);
        row("size", &ui::human_size(app.size));
        row("source", &app.source_url);
        row("sha256", &app.sha256);
        row("executable", &app.exec);
        if let Some(d) = &app.desktop_file {
            row("launcher", d);
        }
        if let Some(b) = &app.bin_link {
            row("command", b);
        }
        if let Some(u) = &app.update_info {
            row("update info", u);
        }
        row("installed", &ui::date(app.installed_at));
        row("updated", &ui::date(app.updated_at));
        if app.locked {
            row("locked", "yes");
        }
        if let Some(prev) = &app.previous_version {
            row("previous", prev);
        }
        return Ok(());
    }

    let (pkg, _) = resolve::listing(ctx, input).await?;
    println!("{}", style(pkg.display_name()).bold());
    if let Some(s) = &pkg.summary {
        println!("{s}");
    }
    println!();
    row("id", &format!("{}/{}", pkg.pubkey.to_npub(), pkg.d));
    row("version", pkg.version.as_deref().unwrap_or("-"));
    row("format", pkg.format.as_deref().unwrap_or("-"));
    if let Some(size) = pkg.size {
        row("size", &ui::human_size(size));
    }
    for url in &pkg.urls {
        row("url", url);
    }
    row("sha256", &pkg.sha256);
    if let Some(w) = &pkg.web {
        row("homepage", w);
    }
    if let Some(l) = &pkg.license {
        row("license", l);
    }
    if !pkg.arch.is_empty() {
        row("arch", &pkg.arch.join(", "));
    }
    if !pkg.topics.is_empty() {
        row("tags", &pkg.topics.join(", "));
    }
    row("published", &ui::date(pkg.created_at));
    if let Some(listing) = ctx
        .db
        .listings()
        .get(db::ListingAddress::new(pkg.address()))?
        && !listing.recommended_by.is_empty()
    {
        row(
            "recommended by",
            &listing
                .recommended_by
                .iter()
                .map(|pk| ui::short_npub(pk))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    if !pkg.description.is_empty() {
        println!("\n{}", pkg.description);
    }
    Ok(())
}

fn search(ctx: &Ctx, query: &str, only_recommended: bool) -> Result<()> {
    let installed: std::collections::HashSet<String> = ctx
        .db
        .apps()
        .all()?
        .into_iter()
        .map(|a| a.address)
        .collect();
    let listings = ctx.db.listings().all()?;

    let mut d_counts: std::collections::HashMap<&str, usize> = Default::default();
    for l in &listings {
        *d_counts.entry(l.d.as_str()).or_default() += 1;
    }

    let mut hits = vec![];
    for listing in &listings {
        if only_recommended && listing.recommended_by.is_empty() {
            continue;
        }
        let Ok((pkg, _)) = nostr::listing_package(listing) else {
            continue;
        };
        let score = if query.is_empty() {
            1
        } else {
            let mut score = 0;
            for word in query.split_whitespace() {
                if pkg.d.to_lowercase() == word {
                    score += 10;
                } else if pkg.d.to_lowercase().contains(word) {
                    score += 5;
                }
                if pkg.display_name().to_lowercase().contains(word) {
                    score += 5;
                }
                if pkg.topics.iter().any(|t| t == word) {
                    score += 3;
                }
                if pkg
                    .summary
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(word)
                {
                    score += 2;
                }
                if pkg.description.to_lowercase().contains(word) {
                    score += 1;
                }
            }
            score
        };
        if score > 0 {
            hits.push((score, listing, pkg));
        }
    }
    hits.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1.recommended_by.len().cmp(&a.1.recommended_by.len()))
            .then(a.2.d.cmp(&b.2.d))
    });

    if hits.is_empty() {
        ui::step("nothing found");
    }
    for (_, listing, pkg) in hits {
        let reference = if d_counts.get(pkg.d.as_str()).copied().unwrap_or(0) > 1 {
            format!("{}/{}", pkg.pubkey.to_npub(), pkg.d)
        } else {
            pkg.d.clone()
        };
        let recs = listing.recommended_by.len();
        println!(
            "{} {}{}{}{}",
            style(&reference).bold(),
            if pkg.name.is_some() {
                format!("{} ", pkg.display_name())
            } else {
                String::new()
            },
            style(pkg.version.as_deref().unwrap_or("")).cyan(),
            if recs > 0 {
                format!(" {}", style(format!("★{recs}")).yellow())
            } else {
                String::new()
            },
            if installed.contains(&listing.address.0) {
                format!(" {}", style("[installed]").green())
            } else {
                String::new()
            },
        );
        let mut line = String::from("    ");
        if let Some(s) = &pkg.summary {
            line.push_str(s);
            line.push_str(" — ");
        }
        line.push_str(&format!("by {}", ui::short_npub(&listing.pubkey)));
        if !pkg.supports_this_arch() {
            line.push_str(&format!(" (only {})", pkg.arch.join(", ")));
        }
        println!("{}", style(line).dim());
    }
    Ok(())
}

fn autoupdate(ctx: &Ctx, action: &str) -> Result<()> {
    let dir = &ctx.paths.systemd_user;
    let service = dir.join("nam-update.service");
    let timer = dir.join("nam-update.timer");
    let systemctl = |args: &[&str]| -> Result<()> {
        let status = std::process::Command::new("systemctl")
            .arg("--user")
            .args(args)
            .status()
            .context("failed to run systemctl")?;
        if !status.success() {
            bail!("systemctl --user {} failed", args.join(" "));
        }
        Ok(())
    };

    match action {
        "enable" => {
            std::fs::create_dir_all(dir)?;
            let exe = std::env::current_exe()?;
            std::fs::write(
                &service,
                format!(
                    "[Unit]\nDescription=update apps installed with nam\nWants=network-online.target\nAfter=network-online.target\n\n[Service]\nType=oneshot\nExecStart=\"{}\" update --yes --quiet\n",
                    exe.display()
                ),
            )?;
            std::fs::write(
                &timer,
                "[Unit]\nDescription=update apps installed with nam\n\n[Timer]\nOnBootSec=10min\nOnUnitActiveSec=12h\nRandomizedDelaySec=30min\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
            )?;
            systemctl(&["daemon-reload"])?;
            systemctl(&["enable", "--now", "nam-update.timer"])?;
            ui::success("apps will be updated twice a day");
        }
        "disable" => {
            let _ = systemctl(&["disable", "--now", "nam-update.timer"]);
            let _ = std::fs::remove_file(&service);
            let _ = std::fs::remove_file(&timer);
            let _ = systemctl(&["daemon-reload"]);
            ui::success("automatic updates are off");
        }
        _ => {
            let _ = systemctl(&["status", "--no-pager", "nam-update.timer"]);
        }
    }
    Ok(())
}
