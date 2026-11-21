mod appimage;
mod db;
mod desktop;
mod download;
mod edit;
mod format;
mod github;
mod install;
mod nostr;
mod paths;
mod resolve;
mod social;
mod transfer;
mod ui;
mod update;
mod zsync;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, Subcommand};
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
    name = "naml",
    version,
    about = "install, update and integrate portable linux apps",
    after_help = "apps can be referred to by name, by id (\"d\" tag), or as <author>/<id> when the id is ambiguous; the author is an npub, nip-05 or hex pubkey for apps from the catalog, and github.com/<owner>, the website's domain or \"local\" for the others"
)]
struct Cli {
    /// answer yes to every question
    #[arg(short, long, global = true)]
    yes: bool,
    /// only print errors
    #[arg(short, long, global = true)]
    quiet: bool,
    /// work on the apps installed for every user of this machine (asks for root with sudo)
    #[arg(long, visible_alias = "global", global = true)]
    system: bool,
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
        #[arg(required = true, add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
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
    /// refresh the catalog from the catalog servers and recommendations
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
    Name {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
        name: Option<String>,
    },
    /// change how an installed app is set up
    Set {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
        #[command(subcommand)]
        what: SetCommand,
    },
    /// move an app between your apps and the ones installed for everyone
    #[command(visible_alias = "mv")]
    Move {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
        /// where it goes
        #[arg(value_enum)]
        to: Space,
    },
    /// the root side of `naml move <app> system`
    #[command(name = "__receive", hide = true)]
    Receive,
    /// the root side of `naml move <app> user`
    #[command(name = "__send", hide = true)]
    Send { app: String },
    /// change an app's entry in the applications menu
    #[command(subcommand, visible_alias = "launcher")]
    Desktop(DesktopCommand),
    /// set your identity: nsec, bunker:// (to publish) or npub/nip-05 (read-only)
    Login { key: String },
    /// forget your identity
    Logout,
    /// show your identity
    Whoami,
    /// manage and publish the apps you recommend
    #[command(subcommand)]
    Recommend(RecommendCommand),
    /// manage and publish the catalog servers you use for apps
    #[command(subcommand)]
    Servers(RelaysCommand),
    /// announce an app in the catalog
    Publish(Box<PublishCommand>),
    /// update apps in the background with a systemd timer
    Autoupdate {
        #[arg(value_parser = ["enable", "disable", "status"])]
        action: String,
    },
    /// print a shell completion script
    #[command(visible_alias = "complete")]
    Completions {
        /// shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Space {
    /// installed for every user of this machine
    #[value(alias = "global")]
    System,
    /// installed just for you
    User,
}

#[derive(Subcommand)]
enum SetCommand {
    /// the program to run, as a path inside the app directory or a file name; without one naml
    /// picks it again
    Exec { path: Option<String> },
    /// the app's id, for apps that didn't come from the catalog
    Id { id: String },
}

#[derive(Subcommand)]
enum DesktopCommand {
    /// print an app's launcher
    Show {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
    },
    /// put an app in the applications menu, even if it doesn't look graphical
    Create {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
    },
    /// set a key of the launcher, like Name, Comment, Categories or Terminal; without a value
    /// your change to it is dropped
    Set {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
        key: String,
        value: Option<String>,
    },
    /// remove a key from the launcher
    Unset {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
        key: String,
    },
    /// edit the launcher in $VISUAL or $EDITOR
    Edit {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
    },
    /// undo every change made to the launcher by hand
    Reset {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
    },
    /// take an app out of the applications menu
    Remove {
        #[arg(add = clap_complete::engine::ArgValueCompleter::new(complete_installed))]
        app: String,
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
    /// publish your catalog servers
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

fn write_completion(shell: clap_complete::Shell) -> Result<()> {
    use std::io::Write as _;

    let name = shell.to_string();
    let shells = clap_complete::env::Shells::builtins();
    let completer = shells
        .completer(&name)
        .with_context(|| format!("unsupported shell {name}"))?;
    let mut script = Vec::new();
    completer.write_registration("COMPLETE", "naml", "naml", "naml", &mut script)?;
    std::io::stdout().write_all(&script)?;
    Ok(())
}

/// completion candidates for an app argument: the names and ids accepted by
/// [`resolve::installed`]
fn complete_installed(
    current: &std::ffi::OsStr,
) -> Vec<clap_complete::engine::CompletionCandidate> {
    use std::collections::{HashMap, HashSet};

    let Ok(paths) = paths::Paths::new(false) else {
        return Vec::new();
    };
    if !paths.db.exists() {
        return Vec::new();
    }
    let Ok(db) = db::Db::open(&paths.db) else {
        return Vec::new();
    };
    let Ok(apps) = db.apps().all() else {
        return Vec::new();
    };

    let current = current.to_string_lossy();
    // a bare "d" tag only resolves when no other installed app uses it
    let d_counts = apps.iter().fold(HashMap::new(), |mut counts, app| {
        *counts.entry(app.d.as_str()).or_insert(0usize) += 1;
        counts
    });

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for app in &apps {
        let help = match app.version.as_deref() {
            Some(version) if !version.is_empty() => format!("{} {version}", app.name),
            _ => app.name.clone(),
        };
        let d = if d_counts.get(app.d.as_str()).copied().unwrap_or(0) > 1 {
            app.qualified()
        } else {
            app.d.clone()
        };
        for value in [app.handle().to_string(), app.slug.0.clone(), d] {
            if value.is_empty()
                || !value.starts_with(current.as_ref())
                || !seen.insert(value.clone())
            {
                continue;
            }
            candidates.push(
                clap_complete::engine::CompletionCandidate::new(value)
                    .help(Some(help.clone().into())),
            );
        }
    }
    candidates.sort_by(|a, b| a.get_value().cmp(b.get_value()));
    candidates
}

#[tokio::main]
async fn main() {
    clap_complete::CompleteEnv::with_factory(Cli::command).complete();
    let cli = Cli::parse();
    ui::set_quiet(cli.quiet);
    if let Command::Completions { shell } = cli.command {
        if let Err(err) = write_completion(shell) {
            eprintln!("{} {err:#}", style("error:").red().bold());
            std::process::exit(1);
        }
        return;
    }
    if cli.system && matches!(cli.command, Command::Move { .. }) {
        eprintln!(
            "{} naml move takes where the app goes instead of --system: `naml move <app> system` or `naml move <app> user`",
            style("error:").red().bold()
        );
        std::process::exit(1);
    }
    if cli.system
        && !transfer::is_root()
        && let Err(err) = without_root(&cli)
    {
        eprintln!("{} {err:#}", style("error:").red().bold());
        std::process::exit(1);
    }
    if let Err(err) = run(cli).await {
        eprintln!("{} {err:#}", style("error:").red().bold());
        std::process::exit(1);
    }
}

/// --system without being root: apps installed for everyone are run as the user, through their
/// command, and everything else starts over with sudo (or doas), as only root can change them
fn without_root(cli: &Cli) -> Result<()> {
    use std::os::unix::process::CommandExt;

    if let Command::Run { app, args } = &cli.command {
        let Some(command) = paths::Paths::system_command(app) else {
            bail!("no app installed for everyone has the command {app} (see `naml --system list`)");
        };
        let err = std::process::Command::new(&command).args(args).exec();
        bail!("failed to run {}: {err}", command.display());
    }

    let exe = std::env::current_exe().context("can't find naml itself")?;
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    for helper in ["sudo", "doas"] {
        let err = std::process::Command::new(helper)
            .arg(&exe)
            .args(&args)
            .exec();
        if err.kind() != std::io::ErrorKind::NotFound {
            bail!("failed to run {helper}: {err}");
        }
    }
    bail!(
        "apps for everyone are managed as root, and neither sudo nor doas is here; run naml as root"
    )
}

async fn run(cli: Cli) -> Result<()> {
    if cli.system {
        // what we install has to be readable by everyone, whatever root's umask is
        // SAFETY: umask only swaps the process' file mode mask
        unsafe { libc::umask(0o022) };
    }
    let paths = paths::Paths::new(cli.system)?;
    let db = db::Db::open(&paths.db).with_context(|| {
        format!(
            "can't open {} (is another naml running?)",
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
    if !matches!(cli.command, Command::Receive | Command::Send { .. }) {
        install::link_old_launchers(&ctx).await;
    }

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
                ui::step("nothing installed yet, try `naml search`");
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
                "{} apps from {} catalog servers, {} recommended by the {} people you follow",
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
                "{} is back at {} (`naml lock {}` keeps it there)",
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
            edit::rename(&ctx, &mut app, name).await?;
            ui::success(format!(
                "{} is now called {}",
                app.qualified(),
                app.handle()
            ));
        }

        Command::Set { app, what } => {
            let mut app = resolve::installed(&ctx, &app).await?;
            match what {
                SetCommand::Exec { path } => {
                    edit::set_exec(&ctx, &mut app, path.as_deref()).await?;
                    ui::success(format!(
                        "{} runs {}{}",
                        app.handle(),
                        app.exec,
                        if path.is_none() {
                            " (picked by naml)"
                        } else {
                            ""
                        }
                    ));
                }
                SetCommand::Id { id } => {
                    edit::set_id(&ctx, &mut app, &id).await?;
                    ui::success(format!("{} is now {}", app.handle(), app.qualified()));
                }
            }
        }

        Command::Move { app, to } => match to {
            Space::System => {
                let app = resolve::installed(&ctx, &app).await?;
                if !ui::confirm(
                    &format!(
                        "install {} for every user instead of just you?",
                        app.handle()
                    ),
                    ctx.yes,
                ) {
                    return Ok(());
                }
                transfer::to_system(&ctx, &app)?;
                ui::success(format!(
                    "{} is installed for everyone now (`naml --system ...` manages it)",
                    app.handle()
                ));
            }
            Space::User => {
                let app = transfer::to_user(&ctx, &app).await?;
                ui::success(format!("{} is installed just for you now", app.handle()));
            }
        },
        Command::Receive => transfer::receive(&ctx).await?,
        Command::Send { app } => transfer::send(&ctx, &app).await?,

        Command::Desktop(cmd) => match cmd {
            DesktopCommand::Show { app } => {
                edit::desktop_show(&resolve::installed(&ctx, &app).await?)?
            }
            DesktopCommand::Create { app } => {
                let mut app = resolve::installed(&ctx, &app).await?;
                edit::desktop_create(&ctx, &mut app).await?;
                ui::success(format!(
                    "{} is in the applications menu, `naml desktop set {} Name ...` changes it",
                    app.handle(),
                    app.handle()
                ));
            }
            DesktopCommand::Set { app, key, value } => {
                let mut app = resolve::installed(&ctx, &app).await?;
                edit::desktop_set(&ctx, &mut app, &key, value.as_deref().map(Some)).await?;
                ui::success(match &value {
                    Some(v) => format!("{key}={v} for {}", app.handle()),
                    None => format!("{key} of {} is back to what it was", app.handle()),
                });
            }
            DesktopCommand::Unset { app, key } => {
                let mut app = resolve::installed(&ctx, &app).await?;
                edit::desktop_set(&ctx, &mut app, &key, Some(None)).await?;
                ui::success(format!("removed {key} from {}", app.handle()));
            }
            DesktopCommand::Edit { app } => {
                let mut app = resolve::installed(&ctx, &app).await?;
                edit::desktop_edit(&ctx, &mut app).await?;
            }
            DesktopCommand::Reset { app } => {
                let mut app = resolve::installed(&ctx, &app).await?;
                edit::desktop_reset(&ctx, &mut app).await?;
                ui::success(format!("the launcher of {} is as it came", app.handle()));
            }
            DesktopCommand::Remove { app } => {
                let mut app = resolve::installed(&ctx, &app).await?;
                edit::desktop_remove(&ctx, &mut app).await?;
                ui::success(format!(
                    "{} is out of the applications menu (`naml desktop create {}` brings it back)",
                    app.handle(),
                    app.handle()
                ));
            }
        },

        Command::Login { key } => {
            let pk = nostr::login(&ctx, &key).await?;
            let (_, how) = social::whoami(&ctx)?.unwrap_or_default();
            ui::success(format!("logged in as {} ({how})", pk.to_npub()));
            ui::step("your follows' recommendations will show up after `naml sync`");
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
                ui::success("added, `naml recommend publish` to share it");
            }
            RecommendCommand::Remove { app } => {
                let address = match resolve::installed(&ctx, &app).await {
                    Ok(installed) => installed.address,
                    Err(_) => resolve::listing(&ctx, &app).await?.0.address(),
                };
                social::set_recommendation(&ctx, &address, false)?;
                ui::success("removed, `naml recommend publish` to share it");
            }
            RecommendCommand::Publish => {
                let n = social::publish_recommendations(&ctx).await?;
                ui::success(format!("published {n} recommendations"));
            }
        },

        Command::Servers(cmd) => match cmd {
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
                    bail!("{url} isn't one of your catalog servers");
                }
                ui::success(format!("removed {url}"));
            }
            RelaysCommand::Publish => {
                let n = social::publish_relays(&ctx).await?;
                ui::success(format!("published {n} catalog servers"));
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

        Command::Completions { .. } => {
            unreachable!("completions are printed before the context is created")
        }
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
        row(
            "executable",
            &match &app.exec_override {
                Some(_) => format!("{} (set by hand)", app.exec),
                None => app.exec.clone(),
            },
        );
        if let Some(d) = &app.desktop_file {
            row("launcher", d);
        }
        if !app.desktop_overrides.is_empty() {
            row("launcher edits", &app.desktop_overrides.join(", "));
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

/// the system timer runs naml as root, so nobody else may be able to replace it
fn only_root_can_change(exe: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    for path in exe.ancestors() {
        let Ok(meta) = std::fs::metadata(path) else {
            continue;
        };
        if meta.uid() != 0 || meta.mode() & 0o022 != 0 {
            bail!(
                "{} could be changed by someone other than root, so it can't update apps as root; \
                 put naml somewhere like /usr/local/bin first",
                exe.display()
            );
        }
    }
    Ok(())
}

fn autoupdate(ctx: &Ctx, action: &str) -> Result<()> {
    let dir = &ctx.paths.systemd;
    let system = ctx.paths.system;
    let service = dir.join("naml-update.service");
    let timer = dir.join("naml-update.timer");
    let systemctl = |args: &[&str]| -> Result<()> {
        let scope = if system { "--system" } else { "--user" };
        let status = std::process::Command::new("systemctl")
            .arg(scope)
            .args(args)
            .status()
            .context("failed to run systemctl")?;
        if !status.success() {
            bail!("systemctl {scope} {} failed", args.join(" "));
        }
        Ok(())
    };

    match action {
        "enable" => {
            std::fs::create_dir_all(dir)?;
            let exe = std::env::current_exe()?;
            if system {
                only_root_can_change(&exe)?;
            }
            std::fs::write(
                &service,
                format!(
                    "[Unit]\nDescription=update apps installed with naml\nWants=network-online.target\nAfter=network-online.target\n\n[Service]\nType=oneshot\nExecStart=\"{}\"{} update --yes --quiet\n",
                    exe.display(),
                    if system { " --system" } else { "" }
                ),
            )?;
            std::fs::write(
                &timer,
                "[Unit]\nDescription=update apps installed with naml\n\n[Timer]\nOnBootSec=10min\nOnUnitActiveSec=12h\nRandomizedDelaySec=30min\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
            )?;
            systemctl(&["daemon-reload"])?;
            systemctl(&["enable", "--now", "naml-update.timer"])?;
            ui::success("apps will be updated twice a day");
        }
        "disable" => {
            let _ = systemctl(&["disable", "--now", "naml-update.timer"]);
            let _ = std::fs::remove_file(&service);
            let _ = std::fs::remove_file(&timer);
            let _ = systemctl(&["daemon-reload"]);
            ui::success("automatic updates are off");
        }
        _ => {
            let _ = systemctl(&["status", "--no-pager", "naml-update.timer"]);
        }
    }
    Ok(())
}
