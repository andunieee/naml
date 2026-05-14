//! reading AppImages without running them: the ELF header tells where the squashfs payload
//! starts and holds the update information, the payload holds the desktop file and icon.

use anyhow::{Context, Result, bail};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

pub struct ElfInfo {
    /// where the ELF runtime ends and the filesystem image starts
    pub payload_offset: u64,
    /// contents of the ".upd_info" section
    pub update_info: Option<String>,
}

pub fn elf_info(path: &Path) -> Result<ElfInfo> {
    let mut file = std::fs::File::open(path)?;
    let mut ident = [0u8; 64];
    file.read_exact(&mut ident)
        .context("file too small to be an AppImage")?;
    if &ident[..4] != b"\x7fELF" {
        bail!("not an ELF file");
    }
    let is64 = match ident[4] {
        1 => false,
        2 => true,
        _ => bail!("unknown ELF class"),
    };
    let le = ident[5] == 1;

    let u16_at = |b: &[u8], o: usize| {
        let a = [b[o], b[o + 1]];
        if le {
            u16::from_le_bytes(a)
        } else {
            u16::from_be_bytes(a)
        }
    };
    let u32_at = |b: &[u8], o: usize| {
        let a: [u8; 4] = b[o..o + 4].try_into().unwrap();
        if le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        }
    };
    let u64_at = |b: &[u8], o: usize| {
        let a: [u8; 8] = b[o..o + 8].try_into().unwrap();
        if le {
            u64::from_le_bytes(a)
        } else {
            u64::from_be_bytes(a)
        }
    };

    let (shoff, shentsize, shnum, shstrndx) = if is64 {
        (
            u64_at(&ident, 0x28),
            u16_at(&ident, 0x3a) as u64,
            u16_at(&ident, 0x3c) as u64,
            u16_at(&ident, 0x3e) as u64,
        )
    } else {
        (
            u32_at(&ident, 0x20) as u64,
            u16_at(&ident, 0x2e) as u64,
            u16_at(&ident, 0x30) as u64,
            u16_at(&ident, 0x32) as u64,
        )
    };
    if shnum == 0 || shentsize == 0 || shnum > 4096 {
        bail!("ELF has no section headers");
    }

    let mut table = vec![0u8; (shentsize * shnum) as usize];
    file.seek(SeekFrom::Start(shoff))?;
    file.read_exact(&mut table)?;

    // (name offset, file offset, size)
    let sections: Vec<(u32, u64, u64)> = (0..shnum as usize)
        .map(|i| {
            let s = &table[i * shentsize as usize..];
            if is64 {
                (u32_at(s, 0), u64_at(s, 0x18), u64_at(s, 0x20))
            } else {
                (u32_at(s, 0), u32_at(s, 0x10) as u64, u32_at(s, 0x14) as u64)
            }
        })
        .collect();

    // same as the AppImage runtime: the payload starts after whichever ends last, the section
    // header table or the last section
    let mut payload_offset = shoff + shentsize * shnum;
    for &(_, offset, size) in &sections {
        payload_offset = payload_offset.max(offset + size);
    }

    let mut update_info = None;
    if let Some(&(_, stroff, strsize)) = sections.get(shstrndx as usize) {
        let mut names = vec![0u8; strsize.min(1 << 20) as usize];
        file.seek(SeekFrom::Start(stroff))?;
        file.read_exact(&mut names)?;
        for &(name, offset, size) in &sections {
            let name_bytes = names.get(name as usize..).unwrap_or_default();
            let end = name_bytes.iter().position(|&b| b == 0).unwrap_or(0);
            if &name_bytes[..end] == b".upd_info" && size > 0 && size < 1 << 16 {
                let mut buf = vec![0u8; size as usize];
                file.seek(SeekFrom::Start(offset))?;
                file.read_exact(&mut buf)?;
                let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
                let text = String::from_utf8_lossy(&buf[..end]).trim().to_string();
                if !text.is_empty() {
                    update_info = Some(text);
                }
            }
        }
    }

    Ok(ElfInfo {
        payload_offset,
        update_info,
    })
}

pub struct Meta {
    pub desktop: Option<String>,
    /// icon bytes and extension
    pub icon: Option<(Vec<u8>, &'static str)>,
}

/// pull the desktop file and icon out of the squashfs payload
pub fn read_meta(path: &Path, payload_offset: u64) -> Result<Meta> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let fs = backhand::FilesystemReader::from_reader_with_offset(reader, payload_offset)
        .map_err(|e| anyhow::anyhow!("can't read the AppImage filesystem: {e}"))?;

    let nodes: Vec<&backhand::Node<backhand::SquashfsFileReader>> = fs.files().collect();
    let find = |p: &Path| {
        nodes
            .iter()
            .find(|n| normalize(&n.fullpath) == normalize(p))
    };

    let read = |start: &Path| -> Option<Vec<u8>> {
        // follow symlinks inside the image, .DirIcon usually is one
        let mut current = normalize(start);
        for _ in 0..8 {
            let node = find(&current)?;
            match &node.inner {
                backhand::InnerNode::File(f) => {
                    let mut buf = vec![];
                    fs.file(f).reader().read_to_end(&mut buf).ok()?;
                    return Some(buf);
                }
                backhand::InnerNode::Symlink(link) => {
                    current = if link.link.is_absolute() {
                        normalize(&link.link)
                    } else {
                        normalize(&current.parent().unwrap_or(Path::new("")).join(&link.link))
                    };
                }
                _ => return None,
            }
        }
        None
    };

    // the desktop file sits at the root
    let desktop_path = nodes
        .iter()
        .map(|n| normalize(&n.fullpath))
        .find(|p| p.components().count() == 1 && p.extension().is_some_and(|e| e == "desktop"));
    let desktop = desktop_path
        .and_then(|p| read(&p))
        .map(|b| String::from_utf8_lossy(&b).into_owned());

    let icon_name = desktop
        .as_deref()
        .and_then(|d| crate::desktop::Desktop::parse(d).get("Icon"))
        .map(|i| i.trim().to_string());

    let mut icon = None;
    if let Some(name) = &icon_name {
        // the root first, then hicolor, the biggest one
        let mut candidates: Vec<(u64, PathBuf)> = nodes
            .iter()
            .filter_map(|n| {
                let p = normalize(&n.fullpath);
                let stem_matches = p.file_stem().is_some_and(|s| s.to_string_lossy() == *name);
                let ext_ok = p
                    .extension()
                    .is_some_and(|e| e == "png" || e == "svg" || e == "xpm");
                if !stem_matches || !ext_ok {
                    return None;
                }
                let size = match &n.inner {
                    backhand::InnerNode::File(f) => f.file_len() as u64,
                    _ => 0,
                };
                Some((crate::format::icon_rank(&p.to_string_lossy(), size), p))
            })
            .collect();
        candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
        for (_, p) in candidates {
            if let Some(bytes) = read(&p) {
                icon = Some(bytes);
                break;
            }
        }
    }
    if icon.is_none() {
        icon = read(Path::new(".DirIcon"));
    }

    Ok(Meta {
        desktop,
        icon: icon.map(|b| {
            let ext = icon_ext(&b);
            (b, ext)
        }),
    })
}

/// fallback for images backhand can't read: let the AppImage's own runtime extract the bits
pub fn read_meta_with_runtime(path: &Path, scratch: &Path) -> Result<Meta> {
    crate::format::make_executable(path)?;
    let run = |pattern: &str| {
        std::process::Command::new(path)
            .arg("--appimage-extract")
            .arg(pattern)
            .current_dir(scratch)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
    };
    run("*.desktop").context("failed to run the AppImage")?;
    run(".DirIcon")?;

    let root = scratch.join("squashfs-root");
    let desktop = std::fs::read_dir(&root)
        .ok()
        .and_then(|mut entries| {
            entries.find_map(|e| {
                let p = e.ok()?.path();
                (p.extension()? == "desktop").then_some(p)
            })
        })
        .and_then(|p| std::fs::read_to_string(p).ok());

    let mut icon_bytes = None;
    if let Some(name) = desktop
        .as_deref()
        .and_then(|d| crate::desktop::Desktop::parse(d).get("Icon"))
    {
        for ext in ["svg", "png"] {
            let _ = run(&format!("{name}.{ext}"));
            if let Ok(bytes) = std::fs::read(root.join(format!("{name}.{ext}"))) {
                icon_bytes = Some(bytes);
                break;
            }
        }
    }
    if icon_bytes.is_none() {
        icon_bytes = std::fs::read(root.join(".DirIcon")).ok();
    }

    Ok(Meta {
        desktop,
        icon: icon_bytes.map(|b| {
            let ext = icon_ext(&b);
            (b, ext)
        }),
    })
}

pub fn icon_ext(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(b"/* XPM") {
        "xpm"
    } else {
        "svg"
    }
}

/// a path inside the image as "a/b/c", without leading slash or dots
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::ParentDir => {
                out.pop();
            }
            _ => {}
        }
    }
    out
}
