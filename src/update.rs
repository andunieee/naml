//! finding and applying updates.
//!
//! apps that came from nostr follow their event: when the publisher replaces it with one
//! pointing to a different file, that's the update. AppImages with update information also get
//! delta downloads through zsync. apps installed straight from a url are updated through their
//! update information if they have it, otherwise by watching the url for changes.

use crate::Ctx;
use crate::db::App;
use crate::download::{self, Downloaded};
use crate::format::Format;
use crate::install;
use crate::nostr::{self, Package};
use crate::ui;
use crate::zsync;
use anyhow::{Context, Result, anyhow, bail};
use ritualistic::{Event, PubKey};

pub enum Plan {
    Nostr {
        pkg: Box<Package>,
        event: Box<Event>,
    },
    Zsync {
        control: Box<zsync::Control>,
    },
    Url,
}

pub struct Available {
    pub app: App,
    pub plan: Plan,
}

impl Available {
    pub fn describe(&self) -> String {
        let from = self.app.version.as_deref().unwrap_or("?");
        match &self.plan {
            Plan::Nostr { pkg, .. } => format!(
                "{} {} → {}",
                self.app.handle(),
                from,
                pkg.version.as_deref().unwrap_or("new version")
            ),
            Plan::Zsync { control } => format!(
                "{} {} → {} (delta update)",
                self.app.handle(),
                from,
                control.filename.as_deref().unwrap_or("new version")
            ),
            Plan::Url => format!(
                "{} {} → whatever is at the url now",
                self.app.handle(),
                from
            ),
        }
    }
}

pub struct CheckResult {
    pub available: Vec<Available>,
    pub locked: Vec<App>,
    pub failed: Vec<(App, anyhow::Error)>,
}

pub async fn check(ctx: &Ctx, apps: Vec<App>) -> Result<CheckResult> {
    let spinner = ui::spinner("checking for updates");
    let wanted: Vec<(PubKey, String)> = apps
        .iter()
        .filter(|a| !a.locked)
        .filter_map(|a| Some((a.pubkey.as_ref()?.parse().ok()?, a.d.clone())))
        .collect();
    let mut latest = if wanted.is_empty() {
        Default::default()
    } else {
        nostr::fetch_apps(ctx, &wanted, &[]).await?
    };

    let mut result = CheckResult {
        available: vec![],
        locked: vec![],
        failed: vec![],
    };
    for app in apps {
        if app.locked {
            result.locked.push(app);
            continue;
        }
        spinner.set_message(format!("checking {}", app.handle()));
        match check_one(ctx, &app, &mut latest).await {
            Ok(Some(plan)) => result.available.push(Available { app, plan }),
            Ok(None) => {}
            Err(err) => result.failed.push((app, err)),
        }
    }
    spinner.finish_and_clear();
    Ok(result)
}

async fn check_one(
    ctx: &Ctx,
    app: &App,
    latest: &mut std::collections::HashMap<String, (Package, Event)>,
) -> Result<Option<Plan>> {
    if app.pubkey.is_some() {
        if let Some((pkg, event)) = latest.remove(&app.address) {
            if pkg.sha256 != app.sha256 && pkg.created_at >= app.event_created_at {
                return Ok(Some(Plan::Nostr {
                    pkg: Box::new(pkg),
                    event: Box::new(event),
                }));
            }
            return Ok(None);
        }
        // the event isn't on our relays anymore; embedded update information may still work
        if app.update_info.is_none() {
            bail!("couldn't find {} on relays", app.qualified());
        }
    }

    if app.format == Format::AppImage.as_str()
        && let Some(info) = &app.update_info
    {
        let zsync_url = resolve_update_info(&ctx.http, info).await?;
        let control = zsync::fetch_control(&ctx.http, &zsync_url).await?;
        if zsync::is_current(&control, std::path::Path::new(&app.exec))? {
            return Ok(None);
        }
        return Ok(Some(Plan::Zsync {
            control: Box::new(control),
        }));
    }

    if app.source_url.starts_with("http") && app.pubkey.is_none() {
        let validator = download::head_validator(&ctx.http, &app.source_url).await?;
        if validator.is_some() && validator == app.http_validator {
            return Ok(None);
        }
        return Ok(Some(Plan::Url));
    }

    Ok(None)
}

/// returns false when it turned out there was nothing new
pub async fn apply(ctx: &Ctx, available: Available) -> Result<bool> {
    let Available { mut app, plan } = available;
    let scratch = ctx.paths.scratch()?;
    let label = app.name.clone();
    let seed = std::path::PathBuf::from(&app.exec);

    let (dl, format, exec_hint) = match &plan {
        Plan::Nostr { pkg, .. } => {
            let format = pkg
                .format
                .as_deref()
                .and_then(Format::parse)
                .or_else(|| Format::parse(&app.format));
            let mut dl = None;
            let info = pkg.update.clone().or(app.update_info.clone());
            if format == Some(Format::AppImage)
                && app.format == Format::AppImage.as_str()
                && let Some(info) = info
            {
                match delta(ctx, &info, &seed, &scratch, &label).await {
                    Ok(d) if d.sha256 == pkg.sha256 => dl = Some(d),
                    Ok(_) => ui::warn(
                        "the delta update doesn't match the published hash, downloading it whole",
                    ),
                    Err(err) => {
                        ui::warn(format!("delta update failed ({err}), downloading it whole"))
                    }
                }
            }
            let dl = match dl {
                Some(dl) => dl,
                None => {
                    download::fetch_any(&ctx.http, &pkg.urls, Some(&pkg.sha256), &scratch, &label)
                        .await?
                }
            };
            (dl, format, pkg.exec.clone())
        }
        Plan::Zsync { control } => {
            let out = scratch.join("download");
            let dl = match zsync::sync(&ctx.http, control, &seed, &out, &label).await {
                Ok(stats) => {
                    ui::step(format!(
                        "reused {}, downloaded {}",
                        ui::human_size(stats.reused),
                        ui::human_size(stats.downloaded)
                    ));
                    downloaded(out)?
                }
                Err(err) => {
                    ui::warn(format!("delta update failed ({err}), downloading it whole"));
                    let dl = download::fetch_any(
                        &ctx.http,
                        std::slice::from_ref(&control.url),
                        None,
                        &scratch,
                        &label,
                    )
                    .await?;
                    if download::sha1_file(&dl.path)? != control.sha1 {
                        bail!("the downloaded file doesn't match its zsync description");
                    }
                    dl
                }
            };
            (dl, Some(Format::AppImage), None)
        }
        Plan::Url => {
            let dl =
                download::fetch_any(&ctx.http, &[app.source_url.clone()], None, &scratch, &label)
                    .await?;
            if dl.sha256 == app.sha256 {
                app.http_validator = dl.validator;
                ctx.db.apps().update(&app)?;
                return Ok(false);
            }
            (dl, None, None)
        }
    };

    let format = match format {
        Some(f) => f,
        None => Format::detect(&dl.path)?,
    };
    let slug = app.slug.0.clone();
    let hints = [slug.as_str(), app.d.as_str(), app.name.as_str()];
    let (staged, exec_rel) = install::stage(
        &dl.path,
        format,
        &scratch,
        &slug,
        exec_hint.as_deref(),
        &hints,
    )?;
    install::replace_files(ctx, &mut app, &staged, &exec_rel, &dl)?;

    app.format = format.as_str().to_string();
    match &plan {
        Plan::Nostr { pkg, event } => {
            app.name = pkg.display_name().to_string();
            app.summary = pkg.summary.clone();
            app.version = pkg.version.clone();
            app.icon_url = pkg.icon.clone();
            app.update_info = pkg.update.clone();
            app.event = serde_json::to_string(event).ok();
            app.event_created_at = pkg.created_at;
        }
        Plan::Zsync { .. } | Plan::Url => {
            // read again from the new file
            app.update_info = None;
            if dl.validator.is_some() {
                app.http_validator = dl.validator.clone();
            }
        }
    }

    let bin = app.bin_link.is_some();
    let from_nostr = matches!(plan, Plan::Nostr { .. });
    install::integrate(ctx, &mut app, bin, !from_nostr, &scratch).await?;
    if !from_nostr && app.version == app.previous_version {
        app.version = None;
    }
    ctx.db.apps().update(&app)?;
    Ok(true)
}

async fn delta(
    ctx: &Ctx,
    info: &str,
    seed: &std::path::Path,
    scratch: &std::path::Path,
    label: &str,
) -> Result<Downloaded> {
    let url = resolve_update_info(&ctx.http, info).await?;
    let control = zsync::fetch_control(&ctx.http, &url).await?;
    let out = scratch.join("download");
    zsync::sync(&ctx.http, &control, seed, &out, label).await?;
    downloaded(out)
}

fn downloaded(path: std::path::PathBuf) -> Result<Downloaded> {
    Ok(Downloaded {
        sha256: download::sha256_file(&path)?,
        size: std::fs::metadata(&path)?.len(),
        path,
        validator: None,
        filename: None,
    })
}

/// AppImageUpdate update information to the url of a .zsync file
pub async fn resolve_update_info(http: &reqwest::Client, info: &str) -> Result<String> {
    let parts: Vec<&str> = info.split('|').collect();
    match parts.as_slice() {
        ["zsync", url] => Ok(url.to_string()),
        ["gh-releases-zsync", owner, repo, tag, pattern] => {
            github_asset(http, owner, repo, tag, pattern).await
        }
        [kind, ..] => bail!("unsupported update information type {kind}"),
        [] => bail!("empty update information"),
    }
}

async fn github_asset(
    http: &reqwest::Client,
    owner: &str,
    repo: &str,
    tag: &str,
    pattern: &str,
) -> Result<String> {
    #[derive(serde::Deserialize)]
    struct Release {
        draft: bool,
        prerelease: bool,
        assets: Vec<Asset>,
    }
    #[derive(serde::Deserialize)]
    struct Asset {
        name: String,
        browser_download_url: String,
    }

    let api = format!("https://api.github.com/repos/{owner}/{repo}/releases");
    let get = |url: String| async move {
        let response = http
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await?;
        if !response.status().is_success() {
            bail!("{url} replied with {}", response.status());
        }
        Ok(response)
    };

    let releases: Vec<Release> = match tag {
        "latest" => vec![get(format!("{api}/latest")).await?.json().await?],
        "latest-pre" | "latest-all" => {
            let all: Vec<Release> = get(format!("{api}?per_page=20")).await?.json().await?;
            all.into_iter()
                .filter(|r| !r.draft && (tag == "latest-all" || r.prerelease))
                .take(1)
                .collect()
        }
        tag => vec![get(format!("{api}/tags/{tag}")).await?.json().await?],
    };

    releases
        .into_iter()
        .flat_map(|r| r.assets)
        .find(|a| glob(pattern, &a.name))
        .map(|a| a.browser_download_url)
        .ok_or_else(|| anyhow!("no asset matching {pattern} in {owner}/{repo} {tag}"))
        .context("resolving gh-releases-zsync")
}

/// '*' and '?' wildcards
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::glob;

    #[test]
    fn globbing() {
        assert!(glob(
            "Foo-*-x86_64.AppImage.zsync",
            "Foo-1.2.3-x86_64.AppImage.zsync"
        ));
        assert!(!glob(
            "Foo-*-x86_64.AppImage.zsync",
            "Foo-1.2.3-aarch64.AppImage.zsync"
        ));
        assert!(glob("*", ""));
        assert!(glob("a?c", "abc"));
    }
}
