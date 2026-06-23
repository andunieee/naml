//! github releases: installing straight from a repository url by picking the release asset that
//! fits this machine, following the repository for updates, and resolving gh-releases-zsync
//! update information.

use anyhow::{Context, Result, anyhow, bail};

/// a repository someone pointed us at, maybe at a specific release
#[derive(Debug, PartialEq)]
pub struct Repo {
    pub owner: String,
    pub repo: String,
    pub tag: Option<String>,
}

impl Repo {
    /// "https://github.com/<owner>/<repo>" and the pages under it, but not direct download
    /// links, those are installed as plain urls
    pub fn parse(input: &str) -> Option<Self> {
        let rest = input
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let rest = rest.strip_prefix("www.").unwrap_or(rest);
        let rest = rest.strip_prefix("github.com/")?;
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
        let (owner, repo) = match parts.as_slice() {
            [owner, repo, ..] => (*owner, repo.trim_end_matches(".git")),
            _ => return None,
        };
        if owner.is_empty() || repo.is_empty() {
            return None;
        }
        let tag = match parts.get(2..) {
            Some(["releases", "download", ..]) => return None,
            Some(["releases", "tag", tag, ..]) => Some(tag.to_string()),
            _ => None,
        };
        Some(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            tag,
        })
    }

    /// what we store as the app's address
    pub fn address(&self) -> String {
        format!(
            "github:{}/{}",
            self.owner.to_lowercase(),
            self.repo.to_lowercase()
        )
    }

    pub fn from_address(address: &str) -> Option<Self> {
        let (owner, repo) = address.strip_prefix("github:")?.split_once('/')?;
        Some(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            tag: None,
        })
    }
}

#[derive(serde::Deserialize)]
pub struct Release {
    pub tag_name: String,
    draft: bool,
    prerelease: bool,
    pub assets: Vec<Asset>,
}

impl Release {
    /// the tag as a version: "v1.2.3" -> "1.2.3"
    pub fn version(&self) -> String {
        let tag = self.tag_name.as_str();
        match tag.strip_prefix('v') {
            Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => rest.to_string(),
            _ => tag.to_string(),
        }
    }
}

#[derive(serde::Deserialize, Clone)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(serde::Deserialize)]
pub struct RepoInfo {
    pub description: Option<String>,
}

/// GET on the api, None on 404. GITHUB_TOKEN lifts the rate limit of 60 requests an hour
async fn get<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    path: &str,
) -> Result<Option<T>> {
    let url = format!("https://api.github.com/{path}");
    let mut request = http
        .get(&url)
        .header("Accept", "application/vnd.github+json");
    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        request = request.bearer_auth(token);
    }
    let response = request.send().await?;
    match response.status() {
        reqwest::StatusCode::NOT_FOUND => Ok(None),
        status if status.is_success() => Ok(Some(response.json().await?)),
        status
            if status == reqwest::StatusCode::FORBIDDEN
                || status == reqwest::StatusCode::TOO_MANY_REQUESTS =>
        {
            bail!("{url} replied with {status}, probably rate limited (setting GITHUB_TOKEN helps)")
        }
        status => bail!("{url} replied with {status}"),
    }
}

/// a release by AppImageUpdate's tag names: "latest" (newest stable), "latest-pre" (newest
/// prerelease), "latest-all" (newest of either), or a tag
pub async fn release(
    http: &reqwest::Client,
    owner: &str,
    repo: &str,
    tag: &str,
) -> Result<Release> {
    let base = format!("repos/{owner}/{repo}/releases");
    let found = match tag {
        "latest" => get(http, &format!("{base}/latest")).await?,
        "latest-pre" | "latest-all" => {
            let all: Vec<Release> = get(http, &format!("{base}?per_page=20"))
                .await?
                .unwrap_or_default();
            all.into_iter()
                .find(|r| !r.draft && (tag == "latest-all" || r.prerelease))
        }
        tag => get(http, &format!("{base}/tags/{tag}")).await?,
    };
    found.ok_or_else(|| anyhow!("{owner}/{repo} has no {tag} release"))
}

/// the release to install from: the one asked for, or the newest stable one, or the newest at
/// all for projects that only publish prereleases
pub async fn release_to_install(http: &reqwest::Client, repo: &Repo) -> Result<Release> {
    if let Some(tag) = &repo.tag {
        return release(http, &repo.owner, &repo.repo, tag).await;
    }
    match release(http, &repo.owner, &repo.repo, "latest").await {
        Ok(r) => Ok(r),
        Err(_) => release(http, &repo.owner, &repo.repo, "latest-all")
            .await
            .with_context(|| format!("{}/{} doesn't publish releases", repo.owner, repo.repo)),
    }
}

pub async fn repo_info(http: &reqwest::Client, repo: &Repo) -> Result<Option<RepoInfo>> {
    get(http, &format!("repos/{}/{}", repo.owner, repo.repo)).await
}

/// gh-releases-zsync update information to the url of the .zsync file
pub async fn zsync_url(
    http: &reqwest::Client,
    owner: &str,
    repo: &str,
    tag: &str,
    pattern: &str,
) -> Result<String> {
    release(http, owner, repo, tag)
        .await?
        .assets
        .into_iter()
        .find(|a| glob(pattern, &a.name))
        .map(|a| a.browser_download_url)
        .ok_or_else(|| anyhow!("no asset matching {pattern} in {owner}/{repo} {tag}"))
        .context("resolving gh-releases-zsync")
}

const ARCHES: &[(&str, &[&str])] = &[
    ("x86_64", &["x86_64", "x86-64", "amd64", "x64"]),
    ("aarch64", &["aarch64", "arm64"]),
    ("arm", &["armv7", "armv6", "armhf", "arm32"]),
    ("x86", &["i386", "i686", "ia32", "x86"]),
    ("riscv64", &["riscv64"]),
];

const OTHER_SYSTEMS: &[&str] = &[
    "windows", "win32", "win64", "macos", "darwin", "osx", "apple", "mac-", "freebsd", "netbsd",
    "openbsd", "android",
];

const ARCHIVES: &[&str] = &[
    ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.zst", ".tar.bz2", ".tar", ".zip",
];

/// files next to the real ones, or packages for other systems
const NOT_APPS: &[&str] = &[
    ".zsync",
    ".sha256",
    ".sha512",
    ".sha1",
    ".md5",
    ".sig",
    ".asc",
    ".pem",
    ".sbom",
    ".json",
    ".txt",
    ".yml",
    ".yaml",
    ".deb",
    ".rpm",
    ".apk",
    ".exe",
    ".msi",
    ".dmg",
    ".pkg",
    ".snap",
    ".flatpak",
    ".7z",
    ".gz",
    ".xz",
    ".zst",
    ".bz2",
    ".blockmap",
    ".appimage.sha256",
];

/// how well an asset fits this machine, None when it doesn't at all
fn score(name: &str, arch: &str) -> Option<u32> {
    let lower = name.to_lowercase();
    let kind = if lower.ends_with(".appimage") {
        3
    } else if ARCHIVES.iter().any(|e| lower.ends_with(e)) {
        2
    } else if lower.contains("linux") && !NOT_APPS.iter().any(|e| lower.ends_with(e)) {
        // a bare executable, like "tool-linux-amd64"
        1
    } else {
        return None;
    };
    if OTHER_SYSTEMS.iter().any(|s| lower.contains(s)) {
        return None;
    }

    let mentions = |a: &str| {
        ARCHES
            .iter()
            .find(|(name, _)| *name == a)
            .is_some_and(|(_, aliases)| aliases.iter().any(|alias| lower.contains(alias)))
    };
    let fit = if mentions(arch) {
        2
    } else if ARCHES.iter().any(|(other, _)| mentions(other)) {
        return None;
    } else {
        1
    };
    // archives that don't say they're for linux are often sources
    if kind == 2 && !lower.contains("linux") && fit == 1 {
        return None;
    }
    Some(kind * 10 + fit * 2 + lower.contains("linux") as u32)
}

/// the file name with versions blurred, to find the same asset in the next release
fn shape(name: &str) -> String {
    let mut out = String::new();
    let mut in_number = false;
    for c in name.chars() {
        if c.is_ascii_digit() || (in_number && c == '.') {
            if !in_number {
                out.push('#');
                in_number = true;
            }
        } else {
            in_number = false;
            out.push(c);
        }
    }
    out
}

/// the assets that could run here, best first
pub fn candidates<'a>(assets: &'a [Asset], arch: &str) -> Vec<(u32, &'a Asset)> {
    let mut found: Vec<(u32, &Asset)> = assets
        .iter()
        .filter_map(|a| Some((score(&a.name, arch)?, a)))
        .collect();
    found.sort_by_key(|(s, _)| std::cmp::Reverse(*s));
    found
}

/// choose what to download from a release. `previous` is the file name installed before, the
/// asset shaped like it wins. otherwise the best fit, asking when several fit equally well
pub fn pick<'a>(
    assets: &'a [Asset],
    previous: Option<&str>,
    interactive: bool,
) -> Result<&'a Asset> {
    let found = candidates(assets, std::env::consts::ARCH);
    if found.is_empty() {
        let names: Vec<&str> = assets.iter().map(|a| a.name.as_str()).collect();
        bail!(
            "nothing in the release looks like it runs on linux {}{}",
            std::env::consts::ARCH,
            if names.is_empty() {
                " (it has no files)".to_string()
            } else {
                format!(", it has: {}", names.join(", "))
            }
        );
    }
    if let Some(previous) = previous {
        let want = shape(previous);
        if let Some((_, a)) = found.iter().find(|(_, a)| shape(&a.name) == want) {
            return Ok(a);
        }
    }
    let best = found[0].0;
    let tied = found.iter().filter(|(s, _)| *s == best).count();
    if tied > 1 && interactive {
        let names: Vec<&str> = found.iter().map(|(_, a)| a.name.as_str()).collect();
        let choice = dialoguer::Select::new()
            .with_prompt("which file?")
            .items(&names)
            .default(0)
            .interact()?;
        return Ok(found[choice].1);
    }
    Ok(found[0].1)
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
    use super::*;

    fn assets(names: &[&str]) -> Vec<Asset> {
        names
            .iter()
            .map(|n| Asset {
                name: n.to_string(),
                browser_download_url: format!("https://example.com/{n}"),
            })
            .collect()
    }

    #[test]
    fn parses_urls() {
        let repo = |owner: &str, name: &str, tag: Option<&str>| {
            Some(Repo {
                owner: owner.into(),
                repo: name.into(),
                tag: tag.map(Into::into),
            })
        };
        assert_eq!(
            Repo::parse("https://github.com/foo/bar"),
            repo("foo", "bar", None)
        );
        assert_eq!(
            Repo::parse("github.com/foo/bar.git"),
            repo("foo", "bar", None)
        );
        assert_eq!(
            Repo::parse("https://github.com/foo/bar/releases/tag/v1.0"),
            repo("foo", "bar", Some("v1.0"))
        );
        assert_eq!(
            Repo::parse("https://github.com/foo/bar/tree/main?tab=readme"),
            repo("foo", "bar", None)
        );
        assert_eq!(
            Repo::parse("https://github.com/foo/bar/releases/download/v1/Bar.AppImage"),
            None
        );
        assert_eq!(Repo::parse("https://github.com/foo"), None);
        assert_eq!(Repo::parse("https://example.com/foo/bar"), None);
    }

    #[test]
    fn prefers_appimages_for_this_arch() {
        let a = assets(&[
            "Foo-1.2.3.dmg",
            "Foo-1.2.3-arm64.AppImage",
            "Foo-1.2.3-x86_64.AppImage",
            "Foo-1.2.3-x86_64.AppImage.zsync",
            "foo-1.2.3-x86_64-linux.tar.gz",
            "Foo-Setup-1.2.3.exe",
            "foo_1.2.3_amd64.deb",
        ]);
        let found = candidates(&a, "x86_64");
        let names: Vec<&str> = found.iter().map(|(_, a)| a.name.as_str()).collect();
        assert_eq!(
            names,
            ["Foo-1.2.3-x86_64.AppImage", "foo-1.2.3-x86_64-linux.tar.gz"]
        );
        assert_eq!(
            candidates(&a, "aarch64")[0].1.name,
            "Foo-1.2.3-arm64.AppImage"
        );
    }

    #[test]
    fn accepts_unlabelled_appimages_and_bare_binaries() {
        let a = assets(&[
            "Foo.AppImage",
            "source.tar.gz",
            "tool-linux-amd64",
            "tool-linux-amd64.sha256",
            "tool-darwin-amd64",
        ]);
        let names: Vec<&str> = candidates(&a, "x86_64")
            .iter()
            .map(|(_, a)| a.name.as_str())
            .collect();
        assert_eq!(names, ["Foo.AppImage", "tool-linux-amd64"]);
    }

    #[test]
    fn follows_the_previous_asset() {
        let a = assets(&[
            "foo-2.0.0-x86_64-unknown-linux-gnu.tar.gz",
            "foo-2.0.0-x86_64-unknown-linux-musl.tar.gz",
        ]);
        let picked = pick(
            &a,
            Some("foo-1.9.10-x86_64-unknown-linux-musl.tar.gz"),
            false,
        )
        .unwrap();
        assert_eq!(picked.name, "foo-2.0.0-x86_64-unknown-linux-musl.tar.gz");
        assert!(pick(&assets(&["notes.txt"]), None, false).is_err());
    }

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
