use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct Paths {
    pub data: PathBuf,
    pub apps: PathBuf,
    pub previous: PathBuf,
    pub icons: PathBuf,
    pub tmp: PathBuf,
    pub db: PathBuf,
    pub applications: PathBuf,
    pub bin: PathBuf,
    pub systemd_user: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().context("can't find the home directory")?;
        let share = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
        let data = share.join("nam");
        let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));

        let paths = Self {
            apps: data.join("apps"),
            previous: data.join("previous"),
            icons: data.join("icons"),
            tmp: data.join("tmp"),
            db: data.join("nam.redb"),
            applications: share.join("applications"),
            bin: dirs::executable_dir().unwrap_or_else(|| home.join(".local/bin")),
            systemd_user: config.join("systemd/user"),
            data,
        };

        for dir in [
            &paths.data,
            &paths.apps,
            &paths.previous,
            &paths.icons,
            &paths.tmp,
            &paths.applications,
            &paths.bin,
        ] {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }

        Ok(paths)
    }

    /// a fresh scratch directory that is removed when the guard is dropped
    pub fn scratch(&self) -> Result<Scratch> {
        let name = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        let dir = self.tmp.join(name);
        std::fs::create_dir_all(&dir)?;
        Ok(Scratch(dir))
    }
}

pub struct Scratch(pub PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
