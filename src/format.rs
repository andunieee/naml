//! the kinds of portable apps we know how to install, and extracting archives

use anyhow::{Context, Result, bail};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    AppImage,
    /// a single executable, usually statically linked
    Binary,
    TarGz,
    TarXz,
    TarZst,
    TarBz2,
    Tar,
    Zip,
}

impl Format {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AppImage => "appimage",
            Self::Binary => "binary",
            Self::TarGz => "tar.gz",
            Self::TarXz => "tar.xz",
            Self::TarZst => "tar.zst",
            Self::TarBz2 => "tar.bz2",
            Self::Tar => "tar",
            Self::Zip => "zip",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_lowercase().as_str() {
            "appimage" => Self::AppImage,
            "binary" | "bin" | "elf" => Self::Binary,
            "tar.gz" | "tgz" => Self::TarGz,
            "tar.xz" | "txz" => Self::TarXz,
            "tar.zst" | "tar.zstd" => Self::TarZst,
            "tar.bz2" | "tbz2" => Self::TarBz2,
            "tar" => Self::Tar,
            "zip" => Self::Zip,
            _ => return None,
        })
    }

    /// sniff the format from the file contents
    pub fn detect(path: &Path) -> Result<Self> {
        let mut head = [0u8; 512];
        let mut file = std::fs::File::open(path)?;
        let n = read_up_to(&mut file, &mut head)?;
        let head = &head[..n];

        if head.starts_with(b"\x7fELF") {
            if head.len() > 10 && &head[8..10] == b"AI" {
                return Ok(Self::AppImage);
            }
            return Ok(Self::Binary);
        }
        if head.starts_with(&[0x1f, 0x8b]) {
            return Ok(Self::TarGz);
        }
        if head.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00]) {
            return Ok(Self::TarXz);
        }
        if head.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
            return Ok(Self::TarZst);
        }
        if head.starts_with(b"BZh") {
            return Ok(Self::TarBz2);
        }
        if head.starts_with(b"PK\x03\x04") {
            return Ok(Self::Zip);
        }
        if head.len() >= 262 && &head[257..262] == b"ustar" {
            return Ok(Self::Tar);
        }
        bail!(
            "don't know how to install this file (it isn't an AppImage, an ELF binary or an archive)"
        )
    }
}

fn read_up_to(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = r.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

/// extract an archive into `dest`, stripping a lone top-level directory
pub fn extract(path: &Path, format: Format, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    match format {
        Format::Zip => {
            let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?)?;
            zip.extract(dest).context("failed to extract zip")?;
        }
        Format::TarGz => untar(flate2::read::GzDecoder::new(reader), dest)?,
        Format::TarXz => untar(liblzma::read::XzDecoder::new(reader), dest)?,
        Format::TarZst => untar(zstd::stream::read::Decoder::new(reader)?, dest)?,
        Format::TarBz2 => untar(bzip2::read::BzDecoder::new(reader), dest)?,
        Format::Tar => untar(reader, dest)?,
        Format::AppImage | Format::Binary => bail!("{} is not an archive", format.as_str()),
    }
    flatten(dest)
}

fn untar(reader: impl Read, dest: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    archive.set_preserve_permissions(true);
    // unpack refuses entries that would land outside dest
    archive.unpack(dest).context("failed to extract tarball")?;
    Ok(())
}

/// "app-1.2/{bin,share}" -> "{bin,share}"
fn flatten(dest: &Path) -> Result<()> {
    let entries: Vec<_> = std::fs::read_dir(dest)?.collect::<Result<_, _>>()?;
    if let [only] = entries.as_slice()
        && only.file_type()?.is_dir()
    {
        let inner = only.path();
        let tmp = dest.with_extension("flatten");
        std::fs::rename(&inner, &tmp)?;
        std::fs::remove_dir(dest)?;
        std::fs::rename(&tmp, dest)?;
    }
    Ok(())
}

pub fn make_executable(path: &Path) -> Result<()> {
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o755);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk(&path, depth + 1, out);
        } else if ft.is_file() {
            out.push(path);
        }
    }
}

fn is_program(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.permissions().mode() & 0o111 == 0 {
        return false;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if name.contains(".so") {
        return false;
    }
    let mut head = [0u8; 4];
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let n = read_up_to(&mut f, &mut head).unwrap_or(0);
    head[..n].starts_with(b"\x7fELF") || head[..n].starts_with(b"#!")
}

/// pick the main executable of an extracted archive
pub fn find_executable(root: &Path, explicit: Option<&str>, hints: &[&str]) -> Result<PathBuf> {
    if let Some(rel) = explicit {
        let path = root.join(rel.trim_start_matches('/'));
        if !path.starts_with(root) || !path.is_file() {
            bail!("the archive has no {rel}");
        }
        return Ok(path);
    }

    let mut files = vec![];
    walk(root, 0, &mut files);
    let hints: Vec<String> = hints.iter().map(|h| h.to_lowercase()).collect();

    let mut best: Option<(i32, PathBuf)> = None;
    for path in files.into_iter().filter(|p| is_program(p)) {
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let name = rel
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let depth = rel.components().count() as i32;

        let mut score = -depth;
        if hints.contains(&name) {
            score += 100;
        } else if hints.iter().any(|h| name.contains(h.as_str())) {
            score += 30;
        }
        if rel.parent().is_some_and(|p| p.ends_with("bin")) {
            score += 10;
        }
        if name.ends_with(".sh") || name.contains("uninstall") || name.contains("crash") {
            score -= 20;
        }
        if best.as_ref().is_none_or(|(s, _)| score > *s) {
            best = Some((score, path));
        }
    }
    best.map(|(_, p)| p).ok_or_else(|| {
        anyhow::anyhow!(
            "couldn't find an executable in the archive (the publisher can set an \"exec\" tag)"
        )
    })
}

/// a .desktop file shipped inside an extracted archive
pub fn find_desktop(root: &Path) -> Option<PathBuf> {
    let mut files = vec![];
    walk(root, 0, &mut files);
    files
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "desktop"))
        .min_by_key(|p| p.components().count())
}

/// the best icon file called `name` (any extension) inside an extracted tree
pub fn find_icon(root: &Path, name: &str) -> Option<PathBuf> {
    let mut files = vec![];
    walk(root, 0, &mut files);
    files
        .into_iter()
        .filter(|p| {
            p.file_stem().is_some_and(|s| s == name)
                && p.extension()
                    .is_some_and(|e| e == "png" || e == "svg" || e == "xpm")
        })
        .max_by_key(|p| {
            icon_rank(
                &p.to_string_lossy(),
                std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
            )
        })
}

/// prefer svg, then the largest png
pub fn icon_rank(path: &str, size: u64) -> u64 {
    if path.ends_with(".svg") {
        u64::MAX
    } else if path.ends_with(".png") {
        size
    } else {
        0
    }
}

/// the architecture an ELF file was built for, named like std::env::consts::ARCH
pub fn elf_arch(path: &Path) -> Option<&'static str> {
    let mut head = [0u8; 20];
    let mut file = std::fs::File::open(path).ok()?;
    if read_up_to(&mut file, &mut head).ok()? < 20 || !head.starts_with(b"\x7fELF") {
        return None;
    }
    let machine = if head[5] == 1 {
        u16::from_le_bytes([head[18], head[19]])
    } else {
        u16::from_be_bytes([head[18], head[19]])
    };
    Some(match machine {
        0x3e => "x86_64",
        0xb7 => "aarch64",
        0x03 => "x86",
        0x28 => "arm",
        0xf3 => "riscv64",
        _ => return None,
    })
}
