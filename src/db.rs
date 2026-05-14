use anyhow::Result;

#[orangedb::schema]
pub mod schema {
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
        match &self.pubkey {
            Some(pk) => format!("{}/{}", crate::ui::npub(pk), self.d),
            None => self.d.clone(),
        }
    }
}
