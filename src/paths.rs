use anyhow::{Context, Result};
use std::path::PathBuf;

/// where naml keeps apps installed for everyone (with --system): the app files under /opt, the
/// database under /var/lib, and commands and launchers under /usr/local like other software
/// installed by hand
const SYSTEM_APPS: &str = "/opt/naml";
const SYSTEM_STATE: &str = "/var/lib/naml";
const SYSTEM_SHARE: &str = "/usr/local/share";
const SYSTEM_BIN: &str = "/usr/local/bin";
const SYSTEM_SYSTEMD: &str = "/etc/systemd/system";

pub struct Paths {
    /// apps are installed for every user of the machine
    pub system: bool,
    pub data: PathBuf,
    pub apps: PathBuf,
    pub previous: PathBuf,
    pub icons: PathBuf,
    pub tmp: PathBuf,
    pub db: PathBuf,
    pub applications: PathBuf,
    pub bin: PathBuf,
    /// where the autoupdate units go: the user's systemd directory, or the system's
    pub systemd: PathBuf,
}

impl Paths {
    pub fn new(system: bool) -> Result<Self> {
        let paths = if system {
            Self::system()
        } else {
            Self::user()?
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

    fn system() -> Self {
        let apps_root = PathBuf::from(SYSTEM_APPS);
        Self {
            system: true,
            data: PathBuf::from(SYSTEM_STATE),
            apps: apps_root.join("apps"),
            previous: apps_root.join("previous"),
            icons: apps_root.join("icons"),
            // next to the apps, so staged apps can be renamed into place
            tmp: apps_root.join("tmp"),
            db: PathBuf::from(SYSTEM_STATE).join("naml.redb"),
            applications: PathBuf::from(SYSTEM_SHARE).join("applications"),
            bin: PathBuf::from(SYSTEM_BIN),
            systemd: PathBuf::from(SYSTEM_SYSTEMD),
        }
    }

    fn user() -> Result<Self> {
        let home = dirs::home_dir().context("can't find the home directory")?;
        let share = dirs::data_dir().unwrap_or_else(|| home.join(".local/share"));
        let data = share.join("naml");
        let config = dirs::config_dir().unwrap_or_else(|| home.join(".config"));

        Ok(Self {
            system: false,
            apps: data.join("apps"),
            previous: data.join("previous"),
            icons: data.join("icons"),
            tmp: data.join("tmp"),
            db: data.join("naml.redb"),
            applications: share.join("applications"),
            bin: dirs::executable_dir().unwrap_or_else(|| home.join(".local/bin")),
            systemd: config.join("systemd/user"),
            data,
        })
    }

    /// the command an app installed for everyone was linked as, found without the database,
    /// which only root can open
    pub fn system_command(name: &str) -> Option<PathBuf> {
        let link = PathBuf::from(SYSTEM_BIN).join(name);
        std::fs::read_link(&link)
            .is_ok_and(|target| target.starts_with(SYSTEM_APPS))
            .then_some(link)
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
