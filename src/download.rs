use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct Downloaded {
    pub path: PathBuf,
    pub sha256: String,
    pub size: u64,
    pub validator: Option<String>,
    pub filename: Option<String>,
}

/// where a file comes from: http(s), file:// or a plain path
pub enum Source {
    Http(String),
    Local(PathBuf),
}

impl Source {
    pub fn parse(input: &str) -> Option<Self> {
        if input.starts_with("http://") || input.starts_with("https://") {
            return Some(Self::Http(input.to_string()));
        }
        if let Some(rest) = input.strip_prefix("file://") {
            return Some(Self::Local(PathBuf::from(rest)));
        }
        let path = if let Some(rest) = input.strip_prefix("~/") {
            dirs::home_dir()?.join(rest)
        } else {
            PathBuf::from(input)
        };
        if path.is_file() {
            return Some(Self::Local(path.canonicalize().unwrap_or(path)));
        }
        None
    }
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("nam/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .expect("http client")
}

/// download (or copy) into `dir`, hashing on the way
pub async fn fetch(
    http: &reqwest::Client,
    source: &Source,
    dir: &Path,
    label: &str,
) -> Result<Downloaded> {
    let dest = dir.join("download");
    let mut file = std::fs::File::create(&dest)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;

    match source {
        Source::Local(path) => {
            let mut input = std::fs::File::open(path)
                .with_context(|| format!("can't open {}", path.display()))?;
            let mut buf = vec![0u8; 1 << 16];
            loop {
                let n = input.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                file.write_all(&buf[..n])?;
                size += n as u64;
            }
            Ok(Downloaded {
                path: dest,
                sha256: hex::encode(hasher.finalize()),
                size,
                validator: None,
                filename: path.file_name().map(|f| f.to_string_lossy().into_owned()),
            })
        }
        Source::Http(url) => {
            let response = http
                .get(url)
                .send()
                .await
                .with_context(|| format!("failed to fetch {url}"))?;
            if !response.status().is_success() {
                bail!("{url} replied with {}", response.status());
            }
            let validator = validator_of(response.headers());
            let filename = filename_of(&response);
            let bar = crate::ui::bytes_bar(response.content_length(), label);

            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.with_context(|| format!("download of {url} failed"))?;
                hasher.update(&chunk);
                file.write_all(&chunk)?;
                size += chunk.len() as u64;
                bar.set_position(size);
            }
            bar.finish_and_clear();
            file.flush()?;

            Ok(Downloaded {
                path: dest,
                sha256: hex::encode(hasher.finalize()),
                size,
                validator,
                filename,
            })
        }
    }
}

/// try each url in turn, checking the hash when we know what it should be
pub async fn fetch_any(
    http: &reqwest::Client,
    urls: &[String],
    expected_sha256: Option<&str>,
    dir: &Path,
    label: &str,
) -> Result<Downloaded> {
    let mut last_err = None;
    for url in urls {
        let Some(source) = Source::parse(url) else {
            last_err = Some(anyhow::anyhow!("unsupported url {url}"));
            continue;
        };
        match fetch(http, &source, dir, label).await {
            Ok(dl) => match expected_sha256 {
                Some(expected) if !expected.eq_ignore_ascii_case(&dl.sha256) => {
                    last_err = Some(anyhow::anyhow!(
                        "hash mismatch for {url}: expected {expected}, got {}",
                        dl.sha256
                    ));
                }
                _ => return Ok(dl),
            },
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("no urls to download from")))
}

/// etag or last-modified, whatever tells us the file changed
pub async fn head_validator(http: &reqwest::Client, url: &str) -> Result<Option<String>> {
    let response = http.head(url).send().await?;
    if !response.status().is_success() {
        bail!("{url} replied with {}", response.status());
    }
    Ok(validator_of(response.headers()))
}

fn validator_of(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get(reqwest::header::ETAG)
        .or_else(|| headers.get(reqwest::header::LAST_MODIFIED))
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
}

fn filename_of(response: &reqwest::Response) -> Option<String> {
    if let Some(cd) = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        && let Some(idx) = cd.find("filename=")
    {
        let name = cd[idx + 9..]
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches('"');
        if !name.is_empty() && !name.contains('/') {
            return Some(name.to_string());
        }
    }
    response
        .url()
        .path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|s| !s.is_empty())
        .map(|s| {
            url::form_urlencoded::parse(s.as_bytes())
                .next()
                .map(|(k, _)| k.into_owned())
                .unwrap_or_else(|| s.to_string())
        })
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha1_file(path: &Path) -> Result<String> {
    use sha1::Sha1;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

/// a stable identifier out of a file name: "Obsidian-1.5.3-x86_64.AppImage" -> "obsidian"
pub fn slug_from_filename(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut stem = lower.as_str();
    for ext in [
        ".appimage",
        ".tar.gz",
        ".tgz",
        ".tar.xz",
        ".txz",
        ".tar.zst",
        ".tar.bz2",
        ".tar",
        ".zip",
    ] {
        if let Some(s) = stem.strip_suffix(ext) {
            stem = s;
            break;
        }
    }
    let mut parts = vec![];
    for part in stem.split(['-', '_', ' ', '.']) {
        // stop at the first thing that looks like a version or an architecture
        let versionish = part.starts_with(|c: char| c.is_ascii_digit())
            || (part.starts_with('v') && part[1..].starts_with(|c: char| c.is_ascii_digit()));
        let archish = matches!(
            part,
            "x86" | "x86_64" | "x64" | "amd64" | "aarch64" | "arm64" | "linux" | "i386" | "i686"
        );
        if versionish || archish {
            break;
        }
        if !part.is_empty() {
            parts.push(part);
        }
    }
    let slug: String = parts
        .join("-")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    if slug.is_empty() { "app".into() } else { slug }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(
            slug_from_filename("Obsidian-1.5.3-x86_64.AppImage"),
            "obsidian"
        );
        assert_eq!(
            slug_from_filename("LibreSprite_x86_64.AppImage"),
            "libresprite"
        );
        assert_eq!(
            slug_from_filename("helix-25.01-x86_64-linux.tar.xz"),
            "helix"
        );
        assert_eq!(slug_from_filename("ripgrep"), "ripgrep");
        assert_eq!(slug_from_filename("Some App-v2.AppImage"), "some-app");
    }
}
