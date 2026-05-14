//! zsync, the delta transfer behind AppImageUpdate: the .zsync control file lists a weak
//! rolling checksum and a truncated md4 for every block of the new file. blocks we already have
//! in the old file are copied locally, the rest is fetched with http range requests.

use anyhow::{Context, Result, bail};
use md4::{Digest, Md4};
use std::collections::HashMap;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

pub struct Control {
    pub blocksize: usize,
    pub length: u64,
    pub seq_matches: usize,
    pub rsum_bytes: usize,
    pub checksum_bytes: usize,
    pub url: String,
    pub sha1: String,
    pub filename: Option<String>,
    // (weak, truncated strong) for every block
    blocks: Vec<(u32, Vec<u8>)>,
}

pub async fn fetch_control(http: &reqwest::Client, zsync_url: &str) -> Result<Control> {
    let response = http.get(zsync_url).send().await?;
    if !response.status().is_success() {
        bail!("{zsync_url} replied with {}", response.status());
    }
    let bytes = response.bytes().await?;
    parse_control(&bytes, zsync_url)
}

pub fn parse_control(bytes: &[u8], zsync_url: &str) -> Result<Control> {
    let header_end = bytes
        .windows(2)
        .position(|w| w == b"\n\n")
        .context("malformed zsync file")?;
    let header = String::from_utf8_lossy(&bytes[..header_end]);
    let mut fields: HashMap<String, String> = HashMap::new();
    for line in header.lines() {
        if let Some((k, v)) = line.split_once(':') {
            // only the first URL counts
            fields
                .entry(k.trim().to_string())
                .or_insert_with(|| v.trim().to_string());
        }
    }
    let get = |k: &str| {
        fields
            .get(k)
            .with_context(|| format!("zsync file has no {k}"))
    };

    let blocksize: usize = get("Blocksize")?.parse()?;
    let length: u64 = get("Length")?.parse()?;
    let lengths: Vec<usize> = get("Hash-Lengths")?
        .split(',')
        .map(|s| s.trim().parse())
        .collect::<Result<_, _>>()?;
    let [seq_matches, rsum_bytes, checksum_bytes] = lengths[..] else {
        bail!("bad Hash-Lengths in zsync file");
    };
    if blocksize == 0 || !(1..=4).contains(&rsum_bytes) || !(1..=16).contains(&checksum_bytes) {
        bail!("unsupported zsync parameters");
    }
    let url = url::Url::parse(zsync_url)?.join(get("URL")?)?.to_string();

    let nblocks = length.div_ceil(blocksize as u64) as usize;
    let data = &bytes[header_end + 2..];
    let entry = rsum_bytes + checksum_bytes;
    if data.len() < nblocks * entry {
        bail!("zsync file is truncated");
    }
    let blocks = (0..nblocks)
        .map(|i| {
            let e = &data[i * entry..(i + 1) * entry];
            // the rsum is stored as big-endian (a, b), keeping only its last rsum_bytes bytes
            let mut full = [0u8; 4];
            full[4 - rsum_bytes..].copy_from_slice(&e[..rsum_bytes]);
            (u32::from_be_bytes(full), e[rsum_bytes..].to_vec())
        })
        .collect();

    Ok(Control {
        blocksize,
        length,
        seq_matches,
        rsum_bytes,
        checksum_bytes,
        url,
        sha1: get("SHA-1")?.to_lowercase(),
        filename: fields.get("Filename").cloned(),
        blocks,
    })
}

#[derive(Clone, Copy)]
struct Rsum {
    a: u16,
    b: u16,
}

impl Rsum {
    fn of(block: &[u8]) -> Self {
        let mut a: u16 = 0;
        let mut b: u16 = 0;
        let mut len = block.len() as u16;
        for &c in block {
            a = a.wrapping_add(c as u16);
            b = b.wrapping_add(len.wrapping_mul(c as u16));
            len = len.wrapping_sub(1);
        }
        Self { a, b }
    }

    fn roll(&mut self, old: u8, new: u8, shift: u32) {
        self.a = self.a.wrapping_add(new as u16).wrapping_sub(old as u16);
        self.b = self
            .b
            .wrapping_add(self.a)
            .wrapping_sub(((old as u32) << shift) as u16);
    }

    fn key(&self, rsum_bytes: usize) -> u32 {
        let full = ((self.a as u32) << 16) | self.b as u32;
        if rsum_bytes >= 4 {
            full
        } else {
            full & ((1u32 << (rsum_bytes * 8)) - 1)
        }
    }
}

pub struct Stats {
    pub reused: u64,
    pub downloaded: u64,
}

/// build the new file at `out` from `seed` plus whatever is missing from the server
pub async fn sync(
    http: &reqwest::Client,
    ctl: &Control,
    seed: &Path,
    out: &Path,
    label: &str,
) -> Result<Stats> {
    let bs = ctl.blocksize;
    if !bs.is_power_of_two() {
        bail!("zsync blocksize is not a power of two");
    }
    let shift = bs.trailing_zeros();

    // pad with a block of zeros like zsync does, the last block of the target is padded too
    let mut old = std::fs::read(seed)?;
    old.extend(std::iter::repeat_n(0u8, bs));

    let mut index: HashMap<u32, Vec<usize>> = HashMap::new();
    for (i, (weak, _)) in ctl.blocks.iter().enumerate() {
        index.entry(*weak).or_default().push(i);
    }
    let strong = |data: &[u8]| -> Vec<u8> {
        let digest = Md4::digest(data);
        digest[..ctl.checksum_bytes].to_vec()
    };

    let mut found: Vec<Option<usize>> = vec![None; ctl.blocks.len()];
    let mut missing = ctl.blocks.len();
    let scan_bar = crate::ui::bytes_bar(Some(old.len() as u64), "scanning");

    let wants_next = ctl.seq_matches > 1;
    let mut pos = 0usize;
    let mut rs = Rsum::of(&old[..bs.min(old.len())]);
    let mut rs_next = if old.len() >= 2 * bs {
        Some(Rsum::of(&old[bs..2 * bs]))
    } else {
        None
    };

    while pos + bs <= old.len() && missing > 0 {
        let mut matched = false;
        if let Some(candidates) = index.get(&rs.key(ctl.rsum_bytes)) {
            let mut sum: Option<Vec<u8>> = None;
            for &i in candidates {
                if found[i].is_some() {
                    continue;
                }
                // with seq_matches=2 the weak sum is short, so ask the next block to match too
                // before paying for md4
                if wants_next && i + 1 < ctl.blocks.len() {
                    match rs_next {
                        Some(next) if next.key(ctl.rsum_bytes) == ctl.blocks[i + 1].0 => {}
                        _ => continue,
                    }
                }
                let sum = sum.get_or_insert_with(|| strong(&old[pos..pos + bs]));
                if *sum == ctl.blocks[i].1 {
                    found[i] = Some(pos);
                    missing -= 1;
                    matched = true;
                }
            }
        }

        let step = if matched { bs } else { 1 };
        for _ in 0..step {
            if pos + bs >= old.len() {
                pos += 1;
                break;
            }
            rs.roll(old[pos], old[pos + bs], shift);
            if let Some(next) = rs_next.as_mut() {
                if pos + 2 * bs < old.len() {
                    next.roll(old[pos + bs], old[pos + 2 * bs], shift);
                } else {
                    rs_next = None;
                }
            }
            pos += 1;
        }
        if pos % (1 << 20) < step {
            scan_bar.set_position(pos as u64);
        }
    }
    scan_bar.finish_and_clear();

    // write what we have, then fill the gaps
    let mut file = std::fs::File::create(out)?;
    file.set_len(ctl.length)?;
    let mut reused = 0u64;
    for (i, at) in found.iter().enumerate() {
        if let Some(at) = at {
            let start = i as u64 * bs as u64;
            let len = (ctl.length - start).min(bs as u64) as usize;
            file.seek(SeekFrom::Start(start))?;
            file.write_all(&old[*at..*at + len])?;
            reused += len as u64;
        }
    }
    drop(old);

    // contiguous runs of missing blocks, merging small gaps to save on requests
    let mut ranges: Vec<(usize, usize)> = vec![];
    for (i, at) in found.iter().enumerate() {
        if at.is_some() {
            continue;
        }
        match ranges.last_mut() {
            Some((_, end)) if i <= *end + 4 => *end = i,
            _ => ranges.push((i, i)),
        }
    }

    let to_fetch: u64 = ranges.iter().map(|(s, e)| range_bytes(ctl, *s, *e).1).sum();
    let bar = crate::ui::bytes_bar(Some(to_fetch), label);
    let mut downloaded = 0u64;
    for (s, e) in ranges {
        let (start, len) = range_bytes(ctl, s, e);
        let response = http
            .get(&ctl.url)
            .header(
                reqwest::header::RANGE,
                format!("bytes={}-{}", start, start + len - 1),
            )
            .send()
            .await?;
        if response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            bail!(
                "server doesn't support range requests ({})",
                response.status()
            );
        }
        let body = response.bytes().await?;
        if body.len() as u64 != len {
            bail!("server sent a range of the wrong size");
        }
        file.seek(SeekFrom::Start(start))?;
        file.write_all(&body)?;
        downloaded += len;
        bar.set_position(downloaded);
    }
    bar.finish_and_clear();
    file.flush()?;
    drop(file);

    let sha1 = crate::download::sha1_file(out)?;
    if sha1 != ctl.sha1 {
        bail!("zsync result doesn't match the expected SHA-1");
    }

    Ok(Stats { reused, downloaded })
}

fn range_bytes(ctl: &Control, first: usize, last: usize) -> (u64, u64) {
    let start = first as u64 * ctl.blocksize as u64;
    let end = ((last as u64 + 1) * ctl.blocksize as u64).min(ctl.length);
    (start, end - start)
}

/// whether `path` is already what the control file describes
pub fn is_current(ctl: &Control, path: &Path) -> Result<bool> {
    Ok(crate::download::sha1_file(path)? == ctl.sha1)
}

#[cfg(test)]
fn sha1_hex(data: &[u8]) -> String {
    hex::encode(<sha1::Sha1 as Digest>::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_matches_direct() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i * 7 + i / 13) as u8).collect();
        let bs = 1024;
        let mut rs = Rsum::of(&data[..bs]);
        for pos in 0..data.len() - bs - 1 {
            rs.roll(data[pos], data[pos + bs], bs.trailing_zeros());
            let direct = Rsum::of(&data[pos + 1..pos + 1 + bs]);
            assert_eq!((rs.a, rs.b), (direct.a, direct.b), "at {pos}");
        }
    }

    /// build a control file the way zsyncmake would, for tests
    pub fn make_control(data: &[u8], bs: usize, url: &str) -> Vec<u8> {
        let (seq, rb, cb) = (2usize, 2usize, 8usize);
        let mut out = format!(
            "zsync: 0.6.2\nFilename: x\nBlocksize: {bs}\nLength: {}\nHash-Lengths: {seq},{rb},{cb}\nURL: {url}\nSHA-1: {}\n\n",
            data.len(),
            sha1_hex(data)
        )
        .into_bytes();
        for chunk in data.chunks(bs) {
            let mut block = chunk.to_vec();
            block.resize(bs, 0);
            let r = Rsum::of(&block);
            let full = (((r.a as u32) << 16) | r.b as u32).to_be_bytes();
            out.extend_from_slice(&full[4 - rb..]);
            out.extend_from_slice(&Md4::digest(&block)[..cb]);
        }
        out
    }

    #[tokio::test]
    async fn delta_against_local_server() {
        // new = old with an insertion in the middle and a changed tail
        let old: Vec<u8> = (0..200_000u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        let mut new = old[..90_000].to_vec();
        new.extend_from_slice(b"something inserted here that shifts everything after it");
        new.extend_from_slice(&old[90_000..180_000]);
        new.extend(std::iter::repeat_n(7u8, 5000));

        let new_ = new.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            loop {
                let (mut sock, _) = listener.accept().await.unwrap();
                let data = new_.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = sock.read(&mut buf).await.unwrap();
                    let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                    let range = req
                        .lines()
                        .find_map(|l| l.strip_prefix("range: bytes="))
                        .map(|r| {
                            let (a, b) = r.trim().split_once('-').unwrap();
                            (a.parse::<usize>().unwrap(), b.parse::<usize>().unwrap())
                        })
                        .unwrap();
                    let body = &data[range.0..=range.1];
                    let head = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    sock.write_all(head.as_bytes()).await.unwrap();
                    sock.write_all(body).await.unwrap();
                });
            }
        });

        let dir = tempdir();
        let seed = dir.join("old");
        let out = dir.join("new");
        std::fs::write(&seed, &old).unwrap();
        let url = format!("http://{addr}/file");
        let ctl = parse_control(&make_control(&new, 2048, &url), &format!("{url}.zsync")).unwrap();

        let stats = sync(&reqwest::Client::new(), &ctl, &seed, &out, "test")
            .await
            .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), new);
        assert!(stats.reused > 150_000, "reused only {}", stats.reused);
        assert!(stats.downloaded < 30_000, "downloaded {}", stats.downloaded);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn tempdir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nam-zsync-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
