use anyhow::{anyhow, Context, Result};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use crate::vault::Account;

/// SecretStore persists secret values in a secure credential store.
pub trait SecretStore {
    /// Returns the value stored for `service` / `account`, or `None` if there is none.
    fn get(&self, service: &str, account: &str) -> Result<Option<String>>;
    /// Stores `value` for `service` / `account`, replacing any existing value.
    fn set(&self, service: &str, account: &str, value: &str) -> Result<()>;
    /// Deletes the value stored for `service` / `account`. Returns false if there was none.
    fn delete(&self, service: &str, account: &str) -> Result<bool>;
}

/// Keychain stores secrets in the platform credential store: the login keychain on macOS
/// and the Secret Service on Linux.
#[derive(Default)]
pub struct Keychain;

impl Keychain {
    /// Creates a new keychain-backed store. The platform store is opened on first use.
    pub fn new() -> Self {
        Self
    }

    /// Returns the keyring entry for `service` / `account`.
    fn entry(service: &str, account: &str) -> Result<keyring_core::Entry> {
        static STORE: OnceLock<Result<(), String>> = OnceLock::new();
        STORE
            .get_or_init(|| open_store().map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| anyhow!("failed to open the credential store: {e}"))?;

        Ok(keyring_core::Entry::new(service, account)?)
    }
}

/// Opens the platform credential store and makes it the keyring default.
fn open_store() -> keyring_core::Result<()> {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new()?;
    #[cfg(target_os = "linux")]
    let store = zbus_secret_service_keyring_store::Store::new()?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        keyring_core::set_default_store(store);
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    Err(keyring_core::Error::NotSupportedByStore(
        "only macOS and Linux are supported".to_string(),
    ))
}

impl SecretStore for Keychain {
    fn get(&self, service: &str, account: &str) -> Result<Option<String>> {
        match Self::entry(service, account)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e).context(format!(
                "failed to read {service}/{account} from the keychain"
            )),
        }
    }

    fn set(&self, service: &str, account: &str, value: &str) -> Result<()> {
        Self::entry(service, account)?
            .set_password(value)
            .with_context(|| format!("failed to write {service}/{account} to the keychain"))
    }

    fn delete(&self, service: &str, account: &str) -> Result<bool> {
        match Self::entry(service, account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring_core::Error::NoEntry) => Ok(false),
            Err(e) => Err(e).context(format!(
                "failed to delete {service}/{account} from the keychain"
            )),
        }
    }
}

/// Cache keeps fetched secrets in a [`SecretStore`] and remembers which profiles were loaded.
///
/// Secrets are stored under the service `op-secrets-<profile>` with the secret name as the
/// account. Loaded profiles are recorded in `<dir>/<profile>.metadata`, one `kind:name` line
/// per secret, so they can be exported on shell startup without contacting 1Password.
pub struct Cache {
    /// Store holding the secret values.
    pub store: Box<dyn SecretStore>,
    /// Directory holding the profile metadata files.
    pub dir: PathBuf,
}

impl Cache {
    /// Creates a cache backed by `store` that keeps its metadata in `dir`.
    pub fn new(store: Box<dyn SecretStore>, dir: &Path) -> Self {
        Self {
            store,
            dir: dir.to_path_buf(),
        }
    }

    /// Returns the store service name used for the secrets of `profile`.
    pub fn service(profile: &str) -> String {
        format!("op-secrets-{profile}")
    }

    /// Returns the cached value of secret `name` in `profile`.
    pub fn get(&self, profile: &str, name: &str) -> Result<Option<String>> {
        self.store.get(&Self::service(profile), name)
    }

    /// Caches `value` as secret `name` in `profile`.
    pub fn set(&self, profile: &str, name: &str, value: &str) -> Result<()> {
        self.store.set(&Self::service(profile), name, value)
    }

    /// Returns the path of the metadata file of `profile`.
    pub fn metadata_path(&self, profile: &str) -> PathBuf {
        self.dir.join(format!("{profile}.metadata"))
    }

    /// Returns the secret names recorded for `profile`, or `None` if it was never loaded.
    pub fn loaded(&self, profile: &str) -> Result<Option<Vec<String>>> {
        let path = self.metadata_path(profile);
        let data = match std::fs::read_to_string(&path) {
            Ok(data) => data,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e).context(format!("failed to read {}", path.display())),
        };

        let names = data
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            // Parse: kind:name (e.g. "env:GITHUB_TOKEN" or "ssh:github-work")
            .filter_map(|line| line.split_once(':').map(|(_, name)| name.to_string()))
            .collect();
        Ok(Some(names))
    }

    /// Records every secret of `account` as loaded.
    pub fn save(&self, account: &Account) -> Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("failed to create {}", self.dir.display()))?;

        let mut data = format!(
            "# zsh-op metadata for profile: {}\n# Format: kind:name\n\n",
            account.name
        );
        for secret in &account.secrets {
            data.push_str(&format!("{}:{}\n", secret.kind, secret.name));
        }

        let path = self.metadata_path(&account.name);
        std::fs::write(&path, data).with_context(|| format!("failed to write {}", path.display()))
    }

    /// Deletes every cached secret of `account`, including secrets that are only recorded in
    /// its metadata, and forgets that it was loaded. Returns the number of deleted secrets.
    pub fn clear(&self, account: &Account) -> Result<usize> {
        let mut names: Vec<String> = account.secrets.iter().map(|s| s.name.clone()).collect();
        for name in self.loaded(&account.name)?.unwrap_or_default() {
            if !names.contains(&name) {
                names.push(name);
            }
        }

        let service = Self::service(&account.name);
        let mut count = 0;
        for name in &names {
            if self.store.delete(&service, name)? {
                count += 1;
            }
        }

        let path = self.metadata_path(&account.name);
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != ErrorKind::NotFound => {
                return Err(e).context(format!("failed to remove {}", path.display()));
            }
            _ => {}
        }
        Ok(count)
    }
}

/// MemoryStore is an in-memory [`SecretStore`] used by tests.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct MemoryStore(
    pub std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<(String, String), String>>>,
);

#[cfg(test)]
impl MemoryStore {
    /// Creates a store holding the given `(service, account, value)` items.
    pub fn with(items: &[(&str, &str, &str)]) -> Self {
        let store = Self::default();
        for (service, account, value) in items {
            store.set(service, account, value).unwrap();
        }
        store
    }

    /// Returns the value stored for `service` / `account`.
    pub fn value(&self, service: &str, account: &str) -> Option<String> {
        self.get(service, account).unwrap()
    }
}

#[cfg(test)]
impl SecretStore for MemoryStore {
    fn get(&self, service: &str, account: &str) -> Result<Option<String>> {
        let key = (service.to_string(), account.to_string());
        Ok(self.0.borrow().get(&key).cloned())
    }

    fn set(&self, service: &str, account: &str, value: &str) -> Result<()> {
        let key = (service.to_string(), account.to_string());
        self.0.borrow_mut().insert(key, value.to_string());
        Ok(())
    }

    fn delete(&self, service: &str, account: &str) -> Result<bool> {
        let key = (service.to_string(), account.to_string());
        Ok(self.0.borrow_mut().remove(&key).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::Config;
    use indoc::indoc;

    fn account() -> Account {
        let config = Config::parse(indoc! {"
            version: 1
            accounts:
              - name: personal
                account: my.1password.com
                secrets:
                  - kind: env
                    name: GITHUB_TOKEN
                    path: op://Personal/GitHub/token
                  - kind: ssh
                    name: my-key
                    path: op://Private/SSH/private key
        "})
        .unwrap();
        config.accounts[0].clone()
    }

    #[test]
    #[ignore = "writes to the real platform keychain"]
    fn keychain_round_trips_values() {
        let keychain = Keychain::new();
        let (service, account) = ("zsh-op-test", "ROUND_TRIP");
        let value = "multi\nline \\ \"quoted\" ✓";

        keychain.set(service, account, value).unwrap();
        let read = keychain.get(service, account);
        let deleted = keychain.delete(service, account);

        assert_eq!(read.unwrap().as_deref(), Some(value));
        assert!(deleted.unwrap());
        assert_eq!(keychain.get(service, account).unwrap(), None);
        assert!(!keychain.delete(service, account).unwrap());
    }

    #[test]
    fn service_uses_profile_prefix() {
        assert_eq!(Cache::service("work"), "op-secrets-work");
    }

    #[test]
    fn get_and_set_use_profile_service() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::default();
        let cache = Cache::new(Box::new(store.clone()), dir.path());

        cache.set("personal", "GITHUB_TOKEN", "brown-fox").unwrap();

        assert_eq!(
            store
                .value("op-secrets-personal", "GITHUB_TOKEN")
                .as_deref(),
            Some("brown-fox")
        );
        assert_eq!(
            cache.get("personal", "GITHUB_TOKEN").unwrap().as_deref(),
            Some("brown-fox")
        );
        assert_eq!(cache.get("work", "GITHUB_TOKEN").unwrap(), None);
    }

    #[test]
    fn loaded_returns_none_for_unloaded_profile() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(Box::new(MemoryStore::default()), dir.path());
        assert_eq!(cache.loaded("personal").unwrap(), None);
    }

    #[test]
    fn save_writes_metadata_that_loaded_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(Box::new(MemoryStore::default()), &dir.path().join("op"));

        cache.save(&account()).unwrap();

        let data = std::fs::read_to_string(cache.metadata_path("personal")).unwrap();
        assert!(data.ends_with("\nenv:GITHUB_TOKEN\nssh:my-key\n"));
        assert_eq!(
            cache.loaded("personal").unwrap(),
            Some(vec!["GITHUB_TOKEN".to_string(), "my-key".to_string()])
        );
    }

    #[test]
    fn loaded_reads_metadata_written_by_the_zsh_plugin() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(Box::new(MemoryStore::default()), dir.path());
        std::fs::write(
            cache.metadata_path("personal"),
            "# zsh-op metadata for profile: personal\n# Format: type:name\n# Generated: Mon\n\nenv:GITHUB_TOKEN\nfile:GCP\n",
        )
        .unwrap();

        assert_eq!(
            cache.loaded("personal").unwrap(),
            Some(vec!["GITHUB_TOKEN".to_string(), "GCP".to_string()])
        );
    }

    #[test]
    fn clear_deletes_configured_and_recorded_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::with(&[
            ("op-secrets-personal", "GITHUB_TOKEN", "a"),
            ("op-secrets-personal", "REMOVED", "b"),
            ("op-secrets-work", "GITHUB_TOKEN", "c"),
        ]);
        let cache = Cache::new(Box::new(store.clone()), dir.path());
        std::fs::write(cache.metadata_path("personal"), "env:REMOVED\n").unwrap();

        assert_eq!(cache.clear(&account()).unwrap(), 2);

        assert_eq!(store.value("op-secrets-personal", "GITHUB_TOKEN"), None);
        assert_eq!(store.value("op-secrets-personal", "REMOVED"), None);
        assert_eq!(
            store.value("op-secrets-work", "GITHUB_TOKEN").as_deref(),
            Some("c")
        );
        assert_eq!(cache.loaded("personal").unwrap(), None);
    }
}
