//! what the user tells the world: the apps they publish, recommend, and the relays they use

use crate::Ctx;
use crate::db::{self, Recommendation, RecommendationAddress, UserRelay, UserRelayUrl};
use crate::format::{self, Format};
use crate::nostr::{self, Package};
use crate::ui;
use anyhow::{Result, anyhow, bail};
use ritualistic::{EventTemplate, Filter, Tags, Timestamp};
use std::collections::BTreeSet;

/// installed nostr apps, plus what was added by hand, minus what was excluded by hand
pub fn recommended(ctx: &Ctx) -> Result<BTreeSet<String>> {
    let mut set: BTreeSet<String> = ctx
        .db
        .apps()
        .all()?
        .into_iter()
        .filter(|a| a.pubkey.is_some())
        .map(|a| a.address)
        .collect();
    for rec in ctx.db.recommendations().all()? {
        if rec.include {
            set.insert(rec.address.0);
        } else {
            set.remove(&rec.address.0);
        }
    }
    Ok(set)
}

pub fn set_recommendation(ctx: &Ctx, address: &str, include: bool) -> Result<()> {
    ctx.db.recommendations().upsert(&Recommendation {
        address: RecommendationAddress::new(address),
        include,
    })?;
    Ok(())
}

pub async fn publish_recommendations(ctx: &Ctx) -> Result<usize> {
    let mut set = recommended(ctx)?;
    let excluded: BTreeSet<String> = ctx
        .db
        .recommendations()
        .all()?
        .into_iter()
        .filter(|r| !r.include)
        .map(|r| r.address.0)
        .collect();

    // keep what we published before from elsewhere (another machine), unless excluded here
    if let Some(me) = nostr::my_pubkey(ctx)? {
        let relays = nostr::base_relays(ctx)?;
        let previous = nostr::fetch(
            &ctx.net,
            &relays,
            Filter {
                kinds: Some(vec![nostr::RECOMMENDED_APPS]),
                authors: Some(vec![me]),
                ..Default::default()
            },
        )
        .await;
        if let Some(list) = previous.into_iter().max_by_key(|e| e.created_at) {
            for tag in list.tags.find_all("a") {
                if let Some((pk, d)) = nostr::parse_app_coordinate(&tag[1]) {
                    let addr = nostr::address(&pk.to_hex(), &d);
                    if !excluded.contains(&addr) {
                        set.insert(addr);
                    }
                }
            }
        }
    }

    let tags: Vec<Vec<String>> = set
        .iter()
        .filter_map(|addr| addr.split_once(':'))
        .map(|(pk, d)| vec!["a".to_string(), nostr::app_coordinate(pk, d)])
        .collect();
    let count = tags.len();
    nostr::publish(
        ctx,
        EventTemplate {
            created_at: Timestamp::now(),
            kind: nostr::RECOMMENDED_APPS,
            tags: Tags(tags),
            content: String::new(),
        },
    )
    .await?;
    Ok(count)
}

pub fn add_relay(ctx: &Ctx, url: &str) -> Result<String> {
    let url = ritualistic::normalize_url(url).map_err(|e| anyhow!("invalid relay url: {e}"))?;
    if !matches!(url.scheme(), "ws" | "wss") {
        bail!("relay urls start with wss://");
    }
    ctx.db.user_relays().upsert(&UserRelay {
        url: UserRelayUrl::new(url.to_string()),
    })?;
    Ok(url.to_string())
}

pub fn remove_relay(ctx: &Ctx, url: &str) -> Result<bool> {
    let url = ritualistic::normalize_url(url)
        .map(|u| u.to_string())
        .unwrap_or_else(|_| url.to_string());
    let key = UserRelayUrl::new(url);
    if ctx.db.user_relays().contains(&key)? {
        ctx.db.user_relays().delete(key)?;
        return Ok(true);
    }
    Ok(false)
}

pub async fn publish_relays(ctx: &Ctx) -> Result<usize> {
    let relays = ctx.db.user_relays().all()?;
    if relays.is_empty() {
        bail!("add some relays first with `nam relays add <url>`");
    }
    let mut tags = vec![
        vec!["d".to_string(), nostr::RELAY_SET_D.to_string()],
        vec!["title".to_string(), "portable linux apps".to_string()],
    ];
    for relay in &relays {
        tags.push(vec!["relay".to_string(), relay.url.0.clone()]);
    }
    nostr::publish(
        ctx,
        EventTemplate {
            created_at: Timestamp::now(),
            kind: nostr::RELAY_SET,
            tags: Tags(tags),
            content: String::new(),
        },
    )
    .await?;
    Ok(relays.len())
}

#[derive(Default)]
pub struct PublishArgs {
    pub urls: Vec<String>,
    pub file: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub version: Option<String>,
    pub icon: Option<String>,
    pub web: Option<String>,
    pub license: Option<String>,
    pub format: Option<String>,
    pub exec: Option<String>,
    pub update: Option<String>,
    pub arch: Vec<String>,
    pub topics: Vec<String>,
    pub description: Option<String>,
    pub dry_run: bool,
}

/// announce an app, filling in what we can from the file itself
pub async fn publish_app(ctx: &Ctx, args: PublishArgs) -> Result<()> {
    if args.urls.is_empty() {
        bail!("give at least one url people can download the app from");
    }
    for url in &args.urls {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            bail!("{url} isn't an http(s) url");
        }
    }

    let scratch = ctx.paths.scratch()?;
    let source = match &args.file {
        Some(file) => {
            crate::download::Source::parse(file).ok_or_else(|| anyhow!("{file} doesn't exist"))?
        }
        None => crate::download::Source::Http(args.urls[0].clone()),
    };
    let dl = crate::download::fetch(&ctx.http, &source, &scratch, "hashing").await?;

    let fmt = match args.format.as_deref() {
        Some(f) => Format::parse(f).ok_or_else(|| anyhow!("unknown format {f}"))?,
        None => Format::detect(&dl.path)?,
    };

    let mut name = args.name;
    let mut summary = args.summary;
    let mut version = args.version;
    let mut update = args.update;
    let mut topics = args.topics;
    let mut arch = args.arch;

    if arch.is_empty()
        && matches!(fmt, Format::AppImage | Format::Binary)
        && let Some(a) = format::elf_arch(&dl.path)
    {
        arch.push(a.to_string());
    }

    if fmt == Format::AppImage {
        let info = crate::appimage::elf_info(&dl.path)?;
        if update.is_none() {
            update = info.update_info;
        }
        if let Ok(meta) = crate::appimage::read_meta(&dl.path, info.payload_offset)
            && let Some(text) = meta.desktop
        {
            let desktop = crate::desktop::Desktop::parse(&text);
            name = name.or(desktop.get("Name"));
            summary = summary.or(desktop.get("Comment"));
            version = version.or(desktop.get("X-AppImage-Version"));
            if topics.is_empty()
                && let Some(categories) = desktop.get("Categories")
            {
                topics = categories
                    .split(';')
                    .filter(|c| !c.is_empty())
                    .map(|c| c.to_lowercase())
                    .collect();
            }
        }
    }

    let d = args.id.unwrap_or_else(|| {
        crate::download::slug_from_filename(
            dl.filename
                .as_deref()
                .unwrap_or_else(|| args.urls[0].rsplit('/').next().unwrap_or("app")),
        )
    });

    let pkg = Package {
        pubkey: nostr::my_pubkey(ctx)?.unwrap_or(ritualistic::PubKey([0; 32])),
        d,
        urls: args.urls,
        sha256: dl.sha256,
        name,
        summary,
        description: args.description.unwrap_or_default(),
        version,
        icon: args.icon,
        web: args.web,
        license: args.license,
        format: Some(fmt.as_str().to_string()),
        exec: args.exec,
        update,
        size: Some(dl.size),
        arch,
        topics,
        created_at: 0,
    };

    let template = pkg.to_template();
    eprintln!("{}", serde_json::to_string_pretty(&template)?);
    if args.dry_run {
        return Ok(());
    }
    if !ui::confirm("publish this?", ctx.yes) {
        bail!("not published");
    }
    let event = nostr::publish(ctx, template).await?;
    ui::success(format!(
        "{} is out, install it with: nam install {}/{}",
        pkg.d,
        event.pubkey.to_npub(),
        pkg.d
    ));
    Ok(())
}

pub fn whoami(ctx: &Ctx) -> Result<Option<(String, &'static str)>> {
    let Some(pk) = db::get_setting(&ctx.db, "pubkey")? else {
        return Ok(None);
    };
    let how = if db::get_setting(&ctx.db, "secret")?.is_some() {
        "secret key"
    } else if db::get_setting(&ctx.db, "bunker")?.is_some() {
        "bunker"
    } else {
        "read-only"
    };
    Ok(Some((ui::npub(&pk), how)))
}
