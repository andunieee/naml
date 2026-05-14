//! turning what the user typed into an app.
//!
//! an app can be referred to by its petname, by its "d" tag when that is unambiguous, or by
//! "<author>/<d>" where the author is an npub, nprofile, hex pubkey or nip-05 address. naddr
//! codes work too.

use crate::Ctx;
use crate::db::{App, Listing};
use crate::nostr::{self, Package};
use anyhow::{Result, anyhow, bail};
use ritualistic::{Event, PubKey};

#[derive(Debug, Clone)]
pub enum Ref {
    /// a petname or a "d" tag
    Plain(String),
    /// an author (not yet resolved) and a "d" tag
    Qualified {
        author: String,
        d: String,
        relays: Vec<String>,
    },
}

impl Ref {
    pub fn parse(input: &str) -> Self {
        let input = input.trim();
        if input.starts_with("naddr1")
            && let Ok(ritualistic::codes::DecodeResult::Address(ptr)) =
                ritualistic::codes::decode(input)
        {
            return Ref::Qualified {
                author: ptr.pubkey.to_hex(),
                d: ptr.identifier,
                relays: ptr.relays,
            };
        }
        if let Some((author, d)) = input.split_once('/')
            && looks_like_author(author)
            && !d.is_empty()
        {
            let relays = match ritualistic::codes::decode(author) {
                Ok(ritualistic::codes::DecodeResult::Profile(p)) => p.relays,
                _ => vec![],
            };
            return Ref::Qualified {
                author: author.to_string(),
                d: d.to_string(),
                relays,
            };
        }
        Ref::Plain(input.to_string())
    }
}

fn looks_like_author(s: &str) -> bool {
    s.starts_with("npub1")
        || s.starts_with("nprofile1")
        || (s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
        || ritualistic::addresses::is_valid_identifier(s)
}

/// find an installed app
pub async fn installed(ctx: &Ctx, input: &str) -> Result<App> {
    let apps = ctx.db.apps().all()?;
    match Ref::parse(input) {
        Ref::Plain(name) => {
            if let Some(app) = ctx.db.apps().by_petname(name.clone())? {
                return Ok(app);
            }
            if let Some(app) = apps.iter().find(|a| a.slug.0 == name) {
                return Ok(app.clone());
            }
            let matches: Vec<&App> = apps.iter().filter(|a| a.d == name).collect();
            match matches.as_slice() {
                [] => bail!("no installed app is called {name} (see `nam list`)"),
                [one] => Ok((*one).clone()),
                many => bail!(
                    "{name} is ambiguous, it could be any of:\n{}",
                    many.iter()
                        .map(|a| format!("  {}", a.qualified()))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            }
        }
        Ref::Qualified { author, d, .. } => {
            let pk = nostr::resolve_pubkey(&author).await?;
            let addr = nostr::address(&pk.to_hex(), &d);
            ctx.db
                .apps()
                .by_address(addr)?
                .ok_or_else(|| anyhow!("{input} is not installed"))
        }
    }
}

/// find an app in the catalog, or on relays when it is fully qualified
pub async fn listing(ctx: &Ctx, input: &str) -> Result<(Package, Event)> {
    match Ref::parse(input) {
        Ref::Plain(d) => {
            nostr::ensure_fresh(ctx).await?;
            let matches: Vec<Listing> = ctx
                .db
                .listings()
                .all()?
                .into_iter()
                .filter(|l| l.d == d)
                .collect();
            match matches.as_slice() {
                [] => bail!("no app called {d} is known; try `nam search {d}` or `nam sync`"),
                [one] => nostr::listing_package(one),
                many => {
                    let mut many = many.to_vec();
                    many.sort_by_key(|l| std::cmp::Reverse(l.recommended_by.len()));
                    bail!(
                        "{d} is published by more than one author, pick one:\n{}",
                        many.iter()
                            .map(|l| {
                                let recs = l.recommended_by.len();
                                format!(
                                    "  {}/{}{}",
                                    crate::ui::npub(&l.pubkey),
                                    l.d,
                                    if recs > 0 {
                                        format!("  (recommended by {recs} follows)")
                                    } else {
                                        String::new()
                                    }
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                }
            }
        }
        Ref::Qualified { author, d, relays } => {
            let pk: PubKey = nostr::resolve_pubkey(&author).await?;
            let addr = nostr::address(&pk.to_hex(), &d);
            // always ask relays for the newest version, fall back to the catalog
            let mut found = nostr::fetch_apps(ctx, &[(pk, d.clone())], &relays).await?;
            if let Some(hit) = found.remove(&addr) {
                return Ok(hit);
            }
            if let Some(listing) = ctx
                .db
                .listings()
                .get(crate::db::ListingAddress::new(addr))?
            {
                return nostr::listing_package(&listing);
            }
            bail!(
                "couldn't find {d} by {} on any relay",
                crate::ui::npub(&pk.to_hex())
            )
        }
    }
}
