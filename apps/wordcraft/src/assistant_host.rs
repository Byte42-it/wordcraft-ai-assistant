//! GeenioAI's (the AI assistant's) host on the desktop: the FANTASTICO account key and the model endpoint.
//!
//! The key is write-only for everything outside this file. [`KeyStore::set`] stores the first one
//! and [`KeyStore::replace`] puts a checked new one in its place; only [`FantasticoHost::client`]
//! reads it back, to call the account service. No command, control method, log line or message
//! shows it. It lives in `<settings dir>/assistant/fantastico.key`.

use std::io::Write;
use std::path::{Path, PathBuf};

use fantastico_sdk::{Fantastico, Options, Surface};
use serde_json::Value;
use wordcraft_fantastico::Streaming;
use wordcraft_ui_egui::assistant::{Account, AssistantEvent, AssistantHost, ModelInfo, Service};

/// The largest key file read back (a key is under a hundred bytes).
const MAX_KEY_BYTES: u64 = 4096;

/// The account service's site for WordCraft: where a key comes from and the credit is topped up.
const SITE: &str = "https://artcraft.fantastico.cc";

pub struct KeyStore {
    path: PathBuf,
}

impl KeyStore {
    pub fn new(settings_dir: &Path) -> Self {
        Self { path: settings_dir.join("assistant").join("fantastico.key") }
    }

    pub fn is_set(&self) -> bool {
        self.path.is_file()
    }

    /// Store `key` if none is stored.
    pub fn set(&self, key: &str) -> Result<(), String> {
        if self.is_set() {
            return Err("A key is already set on this computer".into());
        }
        write_new(&self.path, key)
    }

    /// Put `key` in place of the stored one. It is written to a new file first and then moved
    /// over the old one, so a failure leaves the old key intact.
    pub fn replace(&self, key: &str) -> Result<(), String> {
        let fresh = self.path.with_extension("key.new");
        let _ = std::fs::remove_file(&fresh);
        write_new(&fresh, key)?;
        std::fs::rename(&fresh, &self.path).map_err(|e| {
            let _ = std::fs::remove_file(&fresh);
            format!("The key could not be stored: {e}")
        })
    }

    fn load(&self) -> Result<String, String> {
        let size = std::fs::metadata(&self.path).map_err(|_| "No account key is set".to_string())?.len();
        if size > MAX_KEY_BYTES {
            return Err("The stored account key is not readable".into());
        }
        let key = std::fs::read_to_string(&self.path).map_err(|_| "The stored account key is not readable".to_string())?;
        Ok(key.trim().to_string())
    }
}

/// Write `key` to a file created new (never truncated), readable by this user only where the
/// platform has modes.
fn write_new(path: &Path, key: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("The key could not be stored: {e}"))?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| format!("The key could not be stored: {e}"))?;
    file.write_all(key.as_bytes()).and_then(|()| file.sync_all()).map_err(|e| format!("The key could not be stored: {e}"))
}

pub struct FantasticoHost {
    store: KeyStore,
}

impl FantasticoHost {
    pub fn new(settings_dir: &Path) -> Self {
        Self { store: KeyStore::new(settings_dir) }
    }

    fn options() -> Options {
        Options { title: "WordCraft".into(), referer: "https://github.com/storytold/wordcraft".into(), ..Options::default() }
    }

    /// Check `key` with the account service before anything is stored: a mistyped key is never
    /// kept. Returns the trimmed key and its account.
    fn verify(key: &str) -> Result<(String, Account), String> {
        let key = key.trim().to_string();
        if !key.starts_with("ck-fant-") {
            return Err("This is not an account key: an account key starts with ck-fant-".into());
        }
        let client = Fantastico::new(key.clone(), Self::options()).map_err(|e| e.message)?;
        let config = client.config().map_err(|e| match e.code.as_str() {
            "INVALID_CONTO_KEY" => "FANTASTICO does not recognise this key".to_string(),
            "NETWORK_ERROR" => "FANTASTICO cannot be reached: check the internet connection".to_string(),
            _ => e.message,
        })?;
        Ok((key, Self::account_of(config)))
    }

    fn account_of(config: fantastico_sdk::EdgeConfig) -> Account {
        Account { conto_id: config.account_id, model: config.model, remaining_usd: config.balance.as_ref().map(Self::credit) }
    }

    /// The credit as the user should see it: what is left including the unspent reservations
    /// of live keys (`totalRemaining`, worked out by the account service), so a block set aside
    /// by a key doesn't read as spent. An older server only says what is spendable right now.
    fn credit(b: &fantastico_sdk::Balance) -> f64 {
        b.total_remaining_usd.filter(|t| t.is_finite()).unwrap_or(b.remaining_usd)
    }

    fn client(&self) -> Result<Fantastico, String> {
        Fantastico::new(self.store.load()?, Self::options()).map_err(|e| e.message)
    }
}

impl AssistantHost for FantasticoHost {
    fn service(&self) -> Service {
        Service { name: "FANTASTICO".into(), url: SITE.into(), key_prefix: "ck-fant-".into() }
    }

    fn has_key(&self) -> bool {
        self.store.is_set()
    }

    fn connect(&self, key: String) -> Result<Account, String> {
        if self.store.is_set() {
            return Err("A key is already set on this computer".into());
        }
        let (key, account) = Self::verify(&key)?;
        self.store.set(&key)?;
        Ok(account)
    }

    fn change_key(&self, key: String) -> Result<Account, String> {
        let (key, account) = Self::verify(&key)?;
        self.store.replace(&key)?;
        Ok(account)
    }

    fn models(&self) -> Result<Vec<ModelInfo>, String> {
        let list = self.client()?.models().map_err(|e| e.message)?;
        let price = |m: &Value| m.pointer("/pricing/completion").and_then(|p| p.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| p.as_f64()));
        let mut models: Vec<ModelInfo> = list
            .iter()
            .filter_map(|m| {
                let id = m.get("id").and_then(Value::as_str).filter(|id| !id.is_empty() && ModelInfo::is_chat(id))?;
                Some(ModelInfo {
                    id: id.to_string(),
                    name: m.get("name").and_then(Value::as_str).filter(|n| !n.is_empty()).map(str::to_string),
                    output_usd_per_million: price(m).filter(|p| p.is_finite() && *p >= 0.0).map(|p| p * 1_000_000.0),
                })
            })
            .collect();
        models.sort_by_key(|m| m.name.as_deref().unwrap_or(&m.id).to_lowercase());
        Ok(models)
    }

    fn account(&self) -> Option<Account> {
        let config = self.client().ok()?.config().ok()?;
        Some(Self::account_of(config))
    }

    /// The conversation's endpoint streams its replies (`wordcraft-fantastico`).
    fn provider(&self) -> Result<(Box<dyn wordcraft_assistant::Provider + Send>, String), String> {
        let provider = Streaming::new(self.store.load()?, Self::options()).map_err(|e| e.message)?;
        let client = provider.client();
        let model = match client.surface() {
            Surface::Edge => client.config().map(|c| c.model).unwrap_or_else(|_| fantastico_sdk::DEFAULT_MODEL.to_string()),
            Surface::Runtime => fantastico_sdk::DEFAULT_MODEL.to_string(),
        };
        Ok((Box::new(provider), model))
    }

    fn balance(&self) -> Option<f64> {
        self.client().ok()?.balance().ok().map(|b| Self::credit(&b))
    }

    fn event(&self, event: &AssistantEvent) {
        match event {
            AssistantEvent::KeyRegistered { conto_id } => log::info!("GeenioAI: the key of FANTASTICO account {conto_id} is now stored"),
            AssistantEvent::KeyChanged { conto_id } => {
                log::info!("GeenioAI: the stored key was replaced; it belongs to FANTASTICO account {conto_id}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_set_once_and_replaced_only_on_request() {
        let dir = std::env::temp_dir().join(format!("wordcraft-keystore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = KeyStore::new(&dir);
        assert!(!store.is_set());
        assert!(store.load().is_err());
        store.set("ck-fant-one").unwrap();
        assert!(store.is_set());
        assert!(store.set("ck-fant-two").is_err());
        assert_eq!(store.load().unwrap(), "ck-fant-one");
        // A deliberate change replaces it, leaving no stray file behind.
        store.replace("ck-fant-two").unwrap();
        assert_eq!(store.load().unwrap(), "ck-fant-two");
        assert!(!dir.join("assistant").join("fantastico.key.new").exists());
        // The host refuses a second key before calling anybody.
        let host = FantasticoHost::new(&dir);
        assert!(host.connect("ck-fant-two".into()).unwrap_err().contains("already set"));
        std::fs::remove_dir_all(&dir).unwrap();
        // And a key that isn't an account key is refused without a request.
        let host = FantasticoHost::new(&dir);
        assert!(host.connect("sk-or-abc".into()).is_err());
        assert!(host.change_key("sk-or-abc".into()).is_err());
        assert!(!dir.exists());
        assert_eq!(host.service().key_prefix, "ck-fant-");
    }
}
