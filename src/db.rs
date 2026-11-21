use anyhow::Result;

#[orangedb::schema]
pub mod v1 {
    /// an installed app, before manual edits were kept
    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct App {
        // local identifier used for file names: the petname given at install time, or the "d"
        // tag, suffixed when that is already taken
        #[id(String)]
        pub slug: AppSlug,
        #[unique]
        pub petname: Option<String>,
        // "<pubkey hex>:<d>" for apps that came from nostr, "url:<url>" or "file:<path>" otherwise
        #[unique]
        pub address: String,
        pub pubkey: Option<String>,
        pub d: String,
        pub name: String,
        pub summary: Option<String>,
        pub version: Option<String>,
        pub format: String,
        pub source_url: String,
        pub sha256: String,
        pub size: u64,
        // the directory holding the app files and the executable inside it
        pub root: String,
        pub exec: String,
        pub icon_url: Option<String>,
        pub desktop_file: Option<String>,
        pub icon_file: Option<String>,
        pub bin_link: Option<String>,
        // AppImageUpdate update information, either embedded or given by the nostr event
        pub update_info: Option<String>,
        // the nostr event this was installed from, as json
        pub event: Option<String>,
        pub event_created_at: u64,
        // etag or last-modified of the source url, to detect changes on plain urls
        pub http_validator: Option<String>,
        // the previous version is kept around for rollbacks
        pub previous_sha256: Option<String>,
        pub previous_version: Option<String>,
        // the executable of the previous version, relative to its directory
        pub previous_exec: Option<String>,
        pub locked: bool,
        pub integrate: bool,
        pub installed_at: u64,
        pub updated_at: u64,
    }

    /// an app announcement seen on relays
    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct Listing {
        // "<pubkey hex>:<d>"
        #[id(String)]
        pub address: ListingAddress,
        pub pubkey: String,
        pub d: String,
        pub event: String,
        pub created_at: u64,
        // pubkeys of follows that recommend this app
        pub recommended_by: Vec<String>,
    }

    /// relays the user added; they are used for discovery and published as the user's relay set
    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct UserRelay {
        #[id(String)]
        pub url: UserRelayUrl,
    }

    /// manual edits on top of the recommendations list that is derived from installed apps
    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct Recommendation {
        // "<pubkey hex>:<d>"
        #[id(String)]
        pub address: RecommendationAddress,
        // false means "don't recommend this even though it is installed"
        pub include: bool,
    }

    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct Setting {
        #[id(String)]
        pub key: SettingKey,
        pub value: String,
    }
}

#[orangedb::schema]
pub mod schema {
    use super::v1;

    #[migrate]
    fn app_from_v1(old: v1::App) -> orangedb::Result<App> {
        Ok(App {
            slug: AppSlug(old.slug.0),
            petname: old.petname,
            address: old.address,
            pubkey: old.pubkey,
            d: old.d,
            name: old.name,
            summary: old.summary,
            version: old.version,
            format: old.format,
            source_url: old.source_url,
            sha256: old.sha256,
            size: old.size,
            root: old.root,
            exec: old.exec,
            exec_override: None,
            icon_url: old.icon_url,
            desktop_file: old.desktop_file,
            desktop_wanted: false,
            desktop_overrides: vec![],
            icon_file: old.icon_file,
            bin_link: old.bin_link,
            update_info: old.update_info,
            event: old.event,
            event_created_at: old.event_created_at,
            http_validator: old.http_validator,
            previous_sha256: old.previous_sha256,
            previous_version: old.previous_version,
            previous_exec: old.previous_exec,
            locked: old.locked,
            integrate: old.integrate,
            installed_at: old.installed_at,
            updated_at: old.updated_at,
        })
    }

    /// an installed app
    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct App {
        // local identifier used for file names: the petname given at install time, or the "d"
        // tag, suffixed when that is already taken
        #[id(String)]
        pub slug: AppSlug,
        #[unique]
        pub petname: Option<String>,
        // "<pubkey hex>:<d>" for apps that came from nostr, "url:<url>" or "file:<path>" otherwise
        #[unique]
        pub address: String,
        pub pubkey: Option<String>,
        pub d: String,
        pub name: String,
        pub summary: Option<String>,
        pub version: Option<String>,
        pub format: String,
        pub source_url: String,
        pub sha256: String,
        pub size: u64,
        // the directory holding the app files and the executable inside it
        pub root: String,
        pub exec: String,
        // the executable chosen by hand, relative to the app directory; kept across updates
        pub exec_override: Option<String>,
        pub icon_url: Option<String>,
        pub desktop_file: Option<String>,
        // make a launcher even for apps that don't look graphical
        pub desktop_wanted: bool,
        // hand edits of the launcher's [Desktop Entry] group: "Key=value" sets a key, "-Key"
        // removes it
        pub desktop_overrides: Vec<String>,
        pub icon_file: Option<String>,
        pub bin_link: Option<String>,
        // AppImageUpdate update information, either embedded or given by the nostr event
        pub update_info: Option<String>,
        // the nostr event this was installed from, as json
        pub event: Option<String>,
        pub event_created_at: u64,
        // etag or last-modified of the source url, to detect changes on plain urls
        pub http_validator: Option<String>,
        // the previous version is kept around for rollbacks
        pub previous_sha256: Option<String>,
        pub previous_version: Option<String>,
        // the executable of the previous version, relative to its directory
        pub previous_exec: Option<String>,
        pub locked: bool,
        pub integrate: bool,
        pub installed_at: u64,
        pub updated_at: u64,
    }

    /// an app announcement seen on relays
    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct Listing {
        // "<pubkey hex>:<d>"
        #[id(String)]
        pub address: ListingAddress,
        pub pubkey: String,
        pub d: String,
        pub event: String,
        pub created_at: u64,
        // pubkeys of follows that recommend this app
        pub recommended_by: Vec<String>,
    }

    /// relays the user added; they are used for discovery and published as the user's relay set
    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct UserRelay {
        #[id(String)]
        pub url: UserRelayUrl,
    }

    /// manual edits on top of the recommendations list that is derived from installed apps
    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct Recommendation {
        // "<pubkey hex>:<d>"
        #[id(String)]
        pub address: RecommendationAddress,
        // false means "don't recommend this even though it is installed"
        pub include: bool,
    }

    #[entity]
    #[derive(Debug, Clone, PartialEq)]
    pub struct Setting {
        #[id(String)]
        pub key: SettingKey,
        pub value: String,
    }
}

pub use schema::*;

pub fn get_setting(db: &Db, key: &str) -> Result<Option<String>> {
    Ok(db.settings().get(SettingKey::new(key))?.map(|s| s.value))
}

pub fn set_setting(db: &Db, key: &str, value: impl Into<String>) -> Result<()> {
    db.settings().upsert(&Setting {
        key: SettingKey::new(key),
        value: value.into(),
    })?;
    Ok(())
}

pub fn unset_setting(db: &Db, key: &str) -> Result<()> {
    let key = SettingKey::new(key);
    if db.settings().contains(&key)? {
        db.settings().delete(key)?;
    }
    Ok(())
}

impl App {
    /// how this app should be referred to in messages
    pub fn handle(&self) -> &str {
        self.petname.as_deref().unwrap_or(self.slug.0.as_str())
    }

    pub fn qualified(&self) -> String {
        format!("{}/{}", self.author(), self.d)
    }

    /// who the app comes from: the npub for nostr apps, otherwise where it was installed from
    /// ("github.com/<owner>", the domain of a url, or "local" for files)
    pub fn author(&self) -> String {
        if let Some(pk) = &self.pubkey {
            return crate::ui::npub(pk);
        }
        if let Some(repo) = crate::github::Repo::from_address(&self.address) {
            return format!("github.com/{}", repo.owner);
        }
        if let Some(url) = self.address.strip_prefix("url:")
            && let Ok(url) = url::Url::parse(url)
            && let Some(host) = url.host_str()
        {
            return host.strip_prefix("www.").unwrap_or(host).to_string();
        }
        "local".to_string()
    }
}
