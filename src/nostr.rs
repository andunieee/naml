//! the nostr side: app announcements, recommendation lists, discovery and publishing.
//!
//! an app is announced with a single addressable event of kind 32284. its "d" tag identifies
//! it among the apps of that author, "url" points to the file (repeatable, for mirrors) and "x"
//! is the sha256 of that file. everything else is optional:
//!
//! - "name", "summary", "version", "icon" (an image url), "web" (homepage), "license"
//! - "t": categories/keywords, "arch": supported architectures ("x86_64", "aarch64")
//! - "format": "appimage", "binary", "tar.gz", "tar.xz", "tar.zst", "tar.bz2", "tar" or "zip"
//!   (sniffed from the file when missing)
//! - "exec": for archives, the path of the executable inside it
//! - "update": AppImageUpdate update information, used for delta updates
//! - "size": file size in bytes
//!
//! the content is a free-form description.
//!
//! users recommend apps with a kind 10284 list of "a" tags (NIP-51 style) and relays with a
//! NIP-51 relay set (kind 30002) whose "d" tag is "nam".

use crate::Ctx;
use crate::db::{self, Listing, ListingAddress};
use anyhow::{Context, Result, anyhow, bail};
use ritualistic::{
    Event, EventTemplate, Filter, Finalizer, Kind, Network, Occurrence, PubKey, SecretKey,
    SubscriptionOptions, TagQuery, Tags, Timestamp,
};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

pub const APP: Kind = Kind(32284);
pub const RECOMMENDED_APPS: Kind = Kind(10284);
pub const RELAY_SET: Kind = Kind(30002);
pub const RELAY_SET_D: &str = "nam";
pub const CONTACTS: Kind = Kind(3);

pub const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.primal.net",
    "wss://nostr.mom",
];

const QUERY_TIMEOUT: Duration = Duration::from_secs(12);
// relays recommended by follows we'll also look at, the most popular first
const MAX_FRIEND_RELAYS: usize = 8;

#[derive(Debug, Clone)]
pub struct Package {
    pub pubkey: PubKey,
    pub d: String,
    pub urls: Vec<String>,
    pub sha256: String,
    pub name: Option<String>,
    pub summary: Option<String>,
    pub description: String,
    pub version: Option<String>,
    pub icon: Option<String>,
    pub web: Option<String>,
    pub license: Option<String>,
    pub format: Option<String>,
    pub exec: Option<String>,
    pub update: Option<String>,
    pub size: Option<u64>,
    pub arch: Vec<String>,
    pub topics: Vec<String>,
    pub created_at: u64,
}

impl Package {
    pub fn from_event(event: &Event) -> Result<Self> {
        if event.kind != APP {
            bail!("event is kind {}, not an app ({})", event.kind, APP);
        }
        let tags = &event.tags;
        let one = |key: &str| {
            tags.find(key)
                .map(|t| t[1].trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let many = |key: &str| -> Vec<String> {
            tags.find_all(key)
                .map(|t| t[1].trim().to_string())
                .filter(|v| !v.is_empty())
                .collect()
        };

        let d = tags.get_d();
        if d.is_empty() {
            bail!("app event has no \"d\" tag");
        }
        let urls = many("url");
        if urls.is_empty() {
            bail!("app event {d} has no \"url\" tag");
        }
        let sha256 = one("x")
            .map(|x| x.to_lowercase())
            .filter(|x| x.len() == 64 && x.chars().all(|c| c.is_ascii_hexdigit()))
            .ok_or_else(|| anyhow!("app event {d} has no valid \"x\" (sha256) tag"))?;

        Ok(Self {
            pubkey: event.pubkey,
            d,
            urls,
            sha256,
            name: one("name"),
            summary: one("summary"),
            description: event.content.clone(),
            version: one("version"),
            icon: one("icon"),
            web: one("web"),
            license: one("license"),
            format: one("format"),
            exec: one("exec"),
            update: one("update"),
            size: one("size").and_then(|s| s.parse().ok()),
            arch: many("arch"),
            topics: many("t"),
            created_at: event.created_at.0 as u64,
        })
    }

    pub fn to_template(&self) -> EventTemplate {
        let mut tags: Vec<Vec<String>> = vec![vec!["d".into(), self.d.clone()]];
        for url in &self.urls {
            tags.push(vec!["url".into(), url.clone()]);
        }
        tags.push(vec!["x".into(), self.sha256.clone()]);
        let optional = [
            ("name", &self.name),
            ("summary", &self.summary),
            ("version", &self.version),
            ("icon", &self.icon),
            ("web", &self.web),
            ("license", &self.license),
            ("format", &self.format),
            ("exec", &self.exec),
            ("update", &self.update),
        ];
        for (key, value) in optional {
            if let Some(value) = value {
                tags.push(vec![key.into(), value.clone()]);
            }
        }
        if let Some(size) = self.size {
            tags.push(vec!["size".into(), size.to_string()]);
        }
        for arch in &self.arch {
            tags.push(vec!["arch".into(), arch.clone()]);
        }
        for t in &self.topics {
            tags.push(vec!["t".into(), t.to_lowercase()]);
        }
        if let Some(summary) = &self.summary {
            // NIP-31, so clients that don't know this kind can say something about it
            tags.push(vec!["alt".into(), format!("portable linux app: {summary}")]);
        }

        EventTemplate {
            created_at: Timestamp::now(),
            kind: APP,
            tags: Tags(tags),
            content: self.description.clone(),
        }
    }

    pub fn address(&self) -> String {
        address(&self.pubkey.to_hex(), &self.d)
    }

    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.d)
    }

    /// whether this can run on the current machine, as far as the event tells
    pub fn supports_this_arch(&self) -> bool {
        self.arch.is_empty() || self.arch.iter().any(|a| a == std::env::consts::ARCH)
    }
}

pub fn address(pubkey_hex: &str, d: &str) -> String {
    format!("{pubkey_hex}:{d}")
}

pub fn listing_package(listing: &Listing) -> Result<(Package, Event)> {
    let event: Event = serde_json::from_str(&listing.event)?;
    Ok((Package::from_event(&event)?, event))
}

/// query relays until they all EOSE or the timeout hits
pub async fn fetch(net: &Network, relays: &[String], filter: Filter) -> Vec<Event> {
    if relays.is_empty() {
        return vec![];
    }
    let mut rx = net
        .subscribe(relays, filter, {
            // skip_ids is private, so no struct update syntax
            let mut opts = SubscriptionOptions::default();
            opts.label = Some("nam".into());
            opts
        })
        .await;
    let deadline = tokio::time::sleep(QUERY_TIMEOUT);
    tokio::pin!(deadline);

    let mut seen = HashSet::new();
    let mut out = vec![];
    loop {
        tokio::select! {
            occ = rx.recv() => match occ {
                Some(Occurrence::Event(event, _)) => {
                    if seen.insert(event.id) {
                        out.push(*event);
                    }
                }
                _ => break,
            },
            _ = &mut deadline => break,
        }
    }
    out
}

/// keep only the newest event for each (kind, pubkey, d)
pub fn latest(events: Vec<Event>) -> Vec<Event> {
    let mut by_addr: HashMap<(u16, PubKey, String), Event> = HashMap::new();
    for event in events {
        let key = (event.kind.0, event.pubkey, event.tags.get_d());
        match by_addr.get(&key) {
            Some(existing) if existing.created_at >= event.created_at => {}
            _ => {
                by_addr.insert(key, event);
            }
        }
    }
    by_addr.into_values().collect()
}

/// default relays plus the ones the user added
pub fn base_relays(ctx: &Ctx) -> Result<Vec<String>> {
    let mut relays: Vec<String> = DEFAULT_RELAYS.iter().map(|s| s.to_string()).collect();
    for relay in ctx.db.user_relays().all()? {
        if !relays.contains(&relay.url.0) {
            relays.push(relay.url.0);
        }
    }
    Ok(relays)
}

pub fn my_pubkey(ctx: &Ctx) -> Result<Option<PubKey>> {
    Ok(db::get_setting(&ctx.db, "pubkey")?.and_then(|pk| pk.parse().ok()))
}

pub struct SyncReport {
    pub apps: usize,
    pub follows: usize,
    pub relays: usize,
    pub recommended: usize,
}

/// refresh the local catalog of apps from relays and from the lists of the people we follow
pub async fn sync(ctx: &Ctx) -> Result<SyncReport> {
    let spinner = crate::ui::spinner("looking for apps on relays");
    let mut relays = base_relays(ctx)?;
    let me = my_pubkey(ctx)?;

    // who do we follow?
    let mut follows: Vec<PubKey> = vec![];
    if let Some(me) = me {
        spinner.set_message("fetching follows");
        let contacts = fetch(
            &ctx.net,
            &relays,
            Filter {
                kinds: Some(vec![CONTACTS]),
                authors: Some(vec![me]),
                ..Default::default()
            },
        )
        .await;
        if let Some(list) = contacts.into_iter().max_by_key(|e| e.created_at) {
            follows = list
                .tags
                .find_all("p")
                .filter_map(|t| t[1].parse().ok())
                .collect();
        }
        follows.push(me);
        follows.sort_by_key(|pk| pk.0);
        follows.dedup();
    }

    // what do they recommend?
    let mut recommended: HashMap<String, Vec<String>> = HashMap::new();
    let mut relay_votes: HashMap<String, usize> = HashMap::new();
    if !follows.is_empty() {
        spinner.set_message(format!(
            "fetching recommendations from {} follows",
            follows.len()
        ));
        let mut lists = vec![];
        for chunk in follows.chunks(400) {
            lists.extend(
                fetch(
                    &ctx.net,
                    &relays,
                    Filter {
                        kinds: Some(vec![RECOMMENDED_APPS, RELAY_SET]),
                        authors: Some(chunk.to_vec()),
                        ..Default::default()
                    },
                )
                .await,
            );
        }
        for list in latest(lists) {
            if list.kind == RECOMMENDED_APPS {
                for tag in list.tags.find_all("a") {
                    if let Some((pk, d)) = parse_app_coordinate(&tag[1]) {
                        recommended
                            .entry(address(&pk.to_hex(), &d))
                            .or_default()
                            .push(list.pubkey.to_hex());
                    }
                }
            } else if list.tags.get_d() == RELAY_SET_D {
                for tag in list.tags.find_all("relay") {
                    if let Ok(url) = ritualistic::normalize_url(&tag[1]) {
                        *relay_votes.entry(url.to_string()).or_default() += 1;
                    }
                }
            }
        }
    }

    let mut friend_relays: Vec<(String, usize)> = relay_votes.into_iter().collect();
    friend_relays.sort_by_key(|r| std::cmp::Reverse(r.1));
    for (url, _) in friend_relays.into_iter().take(MAX_FRIEND_RELAYS) {
        if !relays.iter().any(|r| same_relay(r, &url)) {
            relays.push(url);
        }
    }

    // the apps themselves: everything relays have, plus specifically the recommended ones
    spinner.set_message(format!("fetching apps from {} relays", relays.len()));
    let mut events = fetch(
        &ctx.net,
        &relays,
        Filter {
            kinds: Some(vec![APP]),
            limit: Some(2000),
            ..Default::default()
        },
    )
    .await;

    let mut wanted: HashMap<PubKey, Vec<String>> = HashMap::new();
    for addr in recommended.keys() {
        if let Some((pk, d)) = addr.split_once(':')
            && let Ok(pk) = pk.parse::<PubKey>()
        {
            wanted.entry(pk).or_default().push(d.to_string());
        }
    }
    for (pk, ds) in wanted {
        events.extend(
            fetch(
                &ctx.net,
                &relays,
                Filter {
                    kinds: Some(vec![APP]),
                    authors: Some(vec![pk]),
                    tags: Some(vec![TagQuery("d".into(), ds)]),
                    ..Default::default()
                },
            )
            .await,
        );
    }
    spinner.finish_and_clear();

    let events = latest(events);
    let mut listings = vec![];
    for event in events {
        let Ok(pkg) = Package::from_event(&event) else {
            continue;
        };
        let addr = pkg.address();
        listings.push(Listing {
            address: ListingAddress::new(addr.clone()),
            pubkey: pkg.pubkey.to_hex(),
            d: pkg.d.clone(),
            event: serde_json::to_string(&event)?,
            created_at: pkg.created_at,
            recommended_by: recommended.get(&addr).cloned().unwrap_or_default(),
        });
    }

    let report = SyncReport {
        apps: listings.len(),
        follows: follows.len(),
        relays: relays.len(),
        recommended: listings
            .iter()
            .filter(|l| !l.recommended_by.is_empty())
            .count(),
    };

    // replace the whole catalog, so apps that vanished from relays go away too
    ctx.db.write(|tx| {
        for old in tx.listings().all()? {
            tx.listings().delete(old.address)?;
        }
        for listing in &listings {
            tx.listings().insert(listing)?;
        }
        Ok::<_, orangedb::Error>(())
    })?;
    db::set_setting(&ctx.db, "last_sync", crate::ui::now().to_string())?;

    Ok(report)
}

/// sync if the catalog was never fetched or is older than a day
pub async fn ensure_fresh(ctx: &Ctx) -> Result<()> {
    let last = db::get_setting(&ctx.db, "last_sync")?
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    if crate::ui::now().saturating_sub(last) > 86400 {
        let report = sync(ctx).await?;
        crate::ui::step(format!("catalog refreshed: {} apps", report.apps));
    }
    Ok(())
}

/// fetch the newest version of specific apps straight from relays
pub async fn fetch_apps(
    ctx: &Ctx,
    wanted: &[(PubKey, String)],
    extra_relays: &[String],
) -> Result<HashMap<String, (Package, Event)>> {
    let mut relays = base_relays(ctx)?;
    for r in extra_relays {
        if !relays.iter().any(|x| same_relay(x, r)) {
            relays.push(r.clone());
        }
    }
    let mut by_author: HashMap<PubKey, Vec<String>> = HashMap::new();
    for (pk, d) in wanted {
        by_author.entry(*pk).or_default().push(d.clone());
    }

    let mut events = vec![];
    for (pk, ds) in by_author {
        events.extend(
            fetch(
                &ctx.net,
                &relays,
                Filter {
                    kinds: Some(vec![APP]),
                    authors: Some(vec![pk]),
                    tags: Some(vec![TagQuery("d".into(), ds)]),
                    ..Default::default()
                },
            )
            .await,
        );
    }

    let mut out = HashMap::new();
    for event in latest(events) {
        if let Ok(pkg) = Package::from_event(&event) {
            out.insert(pkg.address(), (pkg, event));
        }
    }
    Ok(out)
}

fn same_relay(a: &str, b: &str) -> bool {
    match (ritualistic::normalize_url(a), ritualistic::normalize_url(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// "32284:<pubkey>:<d>"
pub fn parse_app_coordinate(coord: &str) -> Option<(PubKey, String)> {
    let mut parts = coord.splitn(3, ':');
    let kind: u16 = parts.next()?.parse().ok()?;
    if Kind(kind) != APP {
        return None;
    }
    let pk = parts.next()?.parse().ok()?;
    let d = parts.next()?.to_string();
    Some((pk, d))
}

pub fn app_coordinate(pubkey_hex: &str, d: &str) -> String {
    format!("{}:{}:{}", APP.0, pubkey_hex, d)
}

// signing

pub async fn signer(ctx: &Ctx) -> Result<Finalizer> {
    // for CI, where nothing should be stored
    if let Ok(secret) = std::env::var("NAM_SECRET_KEY") {
        let sk: SecretKey = secret
            .trim()
            .parse()
            .map_err(|e| anyhow!("NAM_SECRET_KEY is invalid: {e}"))?;
        return Ok(Finalizer::Plain(sk));
    }
    if let Some(secret) = db::get_setting(&ctx.db, "secret")? {
        let sk: SecretKey = secret
            .parse()
            .map_err(|e| anyhow!("stored key is invalid: {e}"))?;
        return Ok(Finalizer::Plain(sk));
    }
    if let (Some(uri), Some(client)) = (
        db::get_setting(&ctx.db, "bunker")?,
        db::get_setting(&ctx.db, "bunker_client")?,
    ) {
        let client: SecretKey = client.parse().map_err(|e| anyhow!("{e}"))?;
        let url = url::Url::parse(&uri)?;
        let target: PubKey = url
            .host_str()
            .context("bunker uri has no pubkey")?
            .parse()
            .map_err(|e| anyhow!("{e}"))?;
        let relays = url
            .query_pairs()
            .filter(|(k, _)| k == "relay")
            .map(|(_, v)| v.to_string())
            .collect();
        let bunker = ritualistic::bunker_client::BunkerClient::new(
            client,
            target,
            relays,
            ctx.net.clone(),
            None,
        );
        return Ok(Finalizer::Bunker(bunker));
    }
    bail!("you need to `nam login` with a secret key or a bunker to publish")
}

/// sets up the identity used for discovery and publishing
pub async fn login(ctx: &Ctx, input: &str) -> Result<PubKey> {
    let input = input.trim();
    for key in ["secret", "bunker", "bunker_client", "pubkey"] {
        db::unset_setting(&ctx.db, key)?;
    }

    if input.starts_with("bunker://") {
        let client = SecretKey::generate();
        let spinner = crate::ui::spinner("connecting to bunker (approve the request there)");
        let bunker = tokio::time::timeout(
            Duration::from_secs(120),
            ritualistic::bunker_client::BunkerClient::connect(
                client.clone(),
                input,
                ctx.net.clone(),
                None,
            ),
        )
        .await
        .context("timed out connecting to the bunker")?
        .map_err(|e| anyhow!("bunker connection failed: {e}"))?;
        let pk = bunker
            .get_public_key()
            .await
            .map_err(|e| anyhow!("bunker didn't give us a pubkey: {e}"))?;
        spinner.finish_and_clear();
        db::set_setting(&ctx.db, "bunker", input)?;
        db::set_setting(&ctx.db, "bunker_client", client.to_hex())?;
        db::set_setting(&ctx.db, "pubkey", pk.to_hex())?;
        return Ok(pk);
    }

    if input.starts_with("nsec1") {
        let sk: SecretKey = input
            .parse()
            .map_err(|e| anyhow!("invalid secret key: {e}"))?;
        db::set_setting(&ctx.db, "secret", sk.to_hex())?;
        db::set_setting(&ctx.db, "pubkey", sk.pubkey().to_hex())?;
        return Ok(sk.pubkey());
    }

    // read-only (a bare hex is always taken as a pubkey): we can discover through follows but
    // not publish
    let pk = resolve_pubkey(input).await?;
    db::set_setting(&ctx.db, "pubkey", pk.to_hex())?;
    Ok(pk)
}

/// npub, nprofile, hex pubkey or nip-05 address
pub async fn resolve_pubkey(input: &str) -> Result<PubKey> {
    if let Ok(pk) = input.parse::<PubKey>() {
        return Ok(pk);
    }
    if ritualistic::addresses::is_valid_identifier(input) {
        let pointer = ritualistic::addresses::query_identifier(input)
            .await
            .map_err(|e| anyhow!("couldn't resolve {input}: {e}"))?;
        return Ok(pointer.pubkey);
    }
    bail!("{input} is not an npub, nprofile, hex pubkey or nip-05 address")
}

pub async fn publish(ctx: &Ctx, template: EventTemplate) -> Result<Event> {
    let finalizer = signer(ctx).await?;
    let event = finalizer
        .finalize_event(template)
        .await
        .map_err(|e| anyhow!("signing failed: {e}"))?;

    let relays = base_relays(ctx)?;
    let mut net = ctx.net.clone();
    let mut results = net
        .publish_many(
            &relays,
            event.clone(),
            ritualistic::PublishOptions {
                auth_automatically: Some(finalizer),
            },
        )
        .await;

    let mut ok = 0;
    while let Some(result) = results.recv().await {
        match result.error {
            None => {
                ok += 1;
                crate::ui::success(format!("published to {}", result.relay_url));
            }
            Some(err) => crate::ui::warn(format!("{}: {err}", result.relay_url)),
        }
    }
    if ok == 0 {
        bail!("no relay accepted the event");
    }
    Ok(event)
}
