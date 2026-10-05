use anyhow::{anyhow, bail, Context, Result};
use std::{fmt::Display, path::Path};

/// Raw representation of the configuration file, before validation.
#[derive(serde::Deserialize)]
struct Spec {
    /// Version of the configuration format.
    version: Option<serde_yaml_ng::Value>,
    /// 1Password accounts, one per profile.
    #[serde(default)]
    accounts: Vec<AccountSpec>,
}

/// Raw representation of a configured account, before validation.
#[derive(serde::Deserialize)]
struct AccountSpec {
    name: Option<String>,
    account: Option<String>,
    #[serde(default)]
    secrets: Vec<SecretSpec>,
}

/// Raw representation of a configured secret, before validation.
#[derive(serde::Deserialize)]
struct SecretSpec {
    kind: Option<String>,
    name: Option<String>,
    path: Option<String>,
}

/// Validated configuration: the profiles and the secrets they load.
#[derive(Debug, Clone)]
pub struct Config {
    /// Configured accounts, one per profile.
    pub accounts: Vec<Account>,
}

impl Config {
    /// Reads, parses and validates the configuration file at `path`.
    pub fn read_from_file(path: &Path) -> Result<Config> {
        let data = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        Self::parse(&data).with_context(|| format!("invalid config file {}", path.display()))
    }

    /// Parses and validates configuration content.
    pub fn parse(data: &str) -> Result<Config> {
        let spec: Spec = serde_yaml_ng::from_str(data)?;
        Self::try_from(spec)
    }

    /// Returns the account of the given profile.
    pub fn profile(&self, name: &str) -> Result<&Account> {
        self.accounts
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| {
                let names: Vec<&str> = self.accounts.iter().map(|a| a.name.as_str()).collect();
                anyhow!(
                    "profile '{}' not found in config (available profiles: {})",
                    name,
                    names.join(", ")
                )
            })
    }
}

impl TryFrom<Spec> for Config {
    type Error = anyhow::Error;

    fn try_from(spec: Spec) -> Result<Self> {
        let version = match spec.version {
            Some(serde_yaml_ng::Value::Number(n)) => n.to_string(),
            Some(serde_yaml_ng::Value::String(s)) => s,
            Some(_) | None => bail!("config missing 'version' field"),
        };
        if version != "1" {
            bail!("unsupported config version: {version} (supported versions: 1)");
        }
        if spec.accounts.is_empty() {
            bail!("config has no accounts defined");
        }

        let mut accounts = Vec::new();
        for (i, account) in spec.accounts.into_iter().enumerate() {
            let name = account
                .name
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("account at index {i} missing 'name' field"))?;
            // Profile names are used in file paths and keychain service names.
            if name.starts_with('.')
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            {
                bail!("account name '{name}' may only contain letters, digits, '.', '_' and '-'");
            }
            if accounts.iter().any(|a: &Account| a.name == name) {
                bail!("account '{name}' is defined more than once");
            }
            let url = account
                .account
                .filter(|s| !s.is_empty())
                .ok_or_else(|| anyhow!("account '{name}' missing 'account' field"))?;

            let mut secrets = Vec::new();
            for (j, secret) in account.secrets.into_iter().enumerate() {
                let kind = secret.kind.filter(|s| !s.is_empty()).ok_or_else(|| {
                    anyhow!("secret at account '{name}' index {j} missing 'kind' field")
                })?;
                let secret_name = secret.name.filter(|s| !s.is_empty()).ok_or_else(|| {
                    anyhow!("secret at account '{name}' index {j} missing 'name' field")
                })?;
                let kind = kind.parse::<SecretKind>().map_err(|_| {
                    anyhow!(
                        "secret '{secret_name}' has invalid kind: {kind} (valid kinds: env, ssh, file)"
                    )
                })?;
                // Environment and file secrets become shell variables, so their names end up in
                // `export NAME=...` statements that are evaluated by the shell.
                if kind != SecretKind::Ssh && !is_variable_name(&secret_name) {
                    bail!(
                        "{kind} secret '{secret_name}' must use a valid environment variable name"
                    );
                }
                let path = secret.path.filter(|s| !s.is_empty()).ok_or_else(|| {
                    anyhow!("secret '{secret_name}' in account '{name}' missing 'path' field")
                })?;
                if !path.starts_with("op://") {
                    bail!("secret '{secret_name}' has invalid path: {path} (path must start with 'op://')");
                }
                if secrets.iter().any(|s: &Secret| s.name == secret_name) {
                    bail!("secret '{secret_name}' is defined more than once in account '{name}'");
                }

                secrets.push(Secret {
                    kind,
                    name: secret_name,
                    path,
                });
            }

            accounts.push(Account {
                name,
                account: url,
                secrets,
            });
        }

        Ok(Config { accounts })
    }
}

/// Returns true if `name` is a valid shell environment variable name.
fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A profile: a 1Password account and the secrets loaded from it.
#[derive(Debug, Clone)]
pub struct Account {
    /// Profile name.
    pub name: String,
    /// 1Password account URL (e.g. my.1password.com).
    pub account: String,
    /// Secrets loaded by this profile.
    pub secrets: Vec<Secret>,
}

impl Account {
    /// Returns the secret with the given name.
    pub fn secret(&self, name: &str) -> Result<&Secret> {
        self.secrets.iter().find(|s| s.name == name).ok_or_else(|| {
            let names: Vec<&str> = self.secrets.iter().map(|s| s.name.as_str()).collect();
            anyhow!(
                "secret '{}' not found in profile '{}' (available secrets: {})",
                name,
                self.name,
                names.join(", ")
            )
        })
    }
}

/// Kind of a configured secret, which decides how it is loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// Exported as an environment variable.
    Env,
    /// Added to ssh-agent.
    Ssh,
    /// Written to a private file whose path is exported as an environment variable.
    File,
}

impl std::str::FromStr for SecretKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "env" => Ok(Self::Env),
            "ssh" => Ok(Self::Ssh),
            "file" => Ok(Self::File),
            _ => Err(format!("unknown secret kind: {}", s)),
        }
    }
}

impl Display for SecretKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Env => write!(f, "env"),
            Self::Ssh => write!(f, "ssh"),
            Self::File => write!(f, "file"),
        }
    }
}

/// A secret: a name and the 1Password reference it is read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Secret {
    /// How the secret is loaded.
    pub kind: SecretKind,
    /// Variable name for env and file secrets, a label for SSH keys.
    pub name: String,
    /// 1Password secret reference (op://vault/item/field).
    pub path: String,
}

impl Secret {
    /// Returns true for the secrets that become environment variables (env and file).
    pub fn is_variable(&self) -> bool {
        self.kind != SecretKind::Ssh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    const VALID: &str = indoc! {"
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
                path: op://Private/SSH/private key?ssh-format=openssh
              - kind: file
                name: GOOGLE_APPLICATION_CREDENTIALS
                path: op://Personal/GCP/service-account
          - name: work
            account: team.1password.com
    "};

    /// Builds a single-account config around the given secrets YAML block.
    fn with_secrets(secrets: &str) -> String {
        format!("version: 1\naccounts:\n  - name: p\n    account: a.1password.com\n    secrets:\n{secrets}")
    }

    fn parse_err(data: &str) -> String {
        Config::parse(data).unwrap_err().to_string()
    }

    #[test]
    fn parse_accepts_valid_config() {
        let config = Config::parse(VALID).unwrap();
        assert_eq!(config.accounts.len(), 2);

        let personal = config.profile("personal").unwrap();
        assert_eq!(personal.account, "my.1password.com");
        assert_eq!(
            personal.secrets.iter().map(|s| s.kind).collect::<Vec<_>>(),
            vec![SecretKind::Env, SecretKind::Ssh, SecretKind::File]
        );
        assert!(config.profile("work").unwrap().secrets.is_empty());
    }

    #[test]
    fn parse_accepts_quoted_version() {
        let data = VALID.replace("version: 1", "version: \"1\"");
        assert!(Config::parse(&data).is_ok());
    }

    #[test]
    fn parse_fails_when_version_is_missing() {
        let data = VALID.replace("version: 1\n", "");
        assert_eq!(parse_err(&data), "config missing 'version' field");
    }

    #[test]
    fn parse_fails_when_version_is_not_1() {
        let data = VALID.replace("version: 1", "version: 2");
        assert_eq!(
            parse_err(&data),
            "unsupported config version: 2 (supported versions: 1)"
        );
    }

    #[test]
    fn parse_fails_when_accounts_are_empty() {
        assert_eq!(
            parse_err("version: 1\naccounts: []\n"),
            "config has no accounts defined"
        );
    }

    #[test]
    fn parse_fails_when_account_name_is_missing() {
        assert_eq!(
            parse_err("version: 1\naccounts:\n  - account: a.1password.com\n"),
            "account at index 0 missing 'name' field"
        );
    }

    #[test]
    fn parse_fails_when_account_name_is_not_path_safe() {
        assert_eq!(
            parse_err("version: 1\naccounts:\n  - name: ../p\n    account: a.1password.com\n"),
            "account name '../p' may only contain letters, digits, '.', '_' and '-'"
        );
    }

    #[test]
    fn parse_fails_when_account_name_is_duplicated() {
        let account = "  - name: p\n    account: a.1password.com\n";
        assert_eq!(
            parse_err(&format!("version: 1\naccounts:\n{}", account.repeat(2))),
            "account 'p' is defined more than once"
        );
    }

    #[test]
    fn parse_fails_when_account_url_is_missing() {
        assert_eq!(
            parse_err("version: 1\naccounts:\n  - name: p\n"),
            "account 'p' missing 'account' field"
        );
    }

    #[test]
    fn parse_fails_when_secret_kind_is_missing() {
        let data = with_secrets("      - name: A\n        path: op://v/i/f\n");
        assert_eq!(
            parse_err(&data),
            "secret at account 'p' index 0 missing 'kind' field"
        );
    }

    #[test]
    fn parse_fails_when_secret_kind_is_invalid() {
        let data = with_secrets("      - kind: token\n        name: A\n        path: op://v/i/f\n");
        assert_eq!(
            parse_err(&data),
            "secret 'A' has invalid kind: token (valid kinds: env, ssh, file)"
        );
    }

    #[test]
    fn parse_fails_when_secret_name_is_missing() {
        let data = with_secrets("      - kind: env\n        path: op://v/i/f\n");
        assert_eq!(
            parse_err(&data),
            "secret at account 'p' index 0 missing 'name' field"
        );
    }

    #[test]
    fn parse_fails_when_secret_path_is_missing() {
        let data = with_secrets("      - kind: env\n        name: A\n");
        assert_eq!(
            parse_err(&data),
            "secret 'A' in account 'p' missing 'path' field"
        );
    }

    #[test]
    fn parse_fails_when_secret_path_is_not_an_op_reference() {
        let data = with_secrets("      - kind: env\n        name: A\n        path: vault/item\n");
        assert_eq!(
            parse_err(&data),
            "secret 'A' has invalid path: vault/item (path must start with 'op://')"
        );
    }

    #[test]
    fn parse_fails_when_file_secret_name_is_not_a_variable_name() {
        let data =
            with_secrets("      - kind: file\n        name: gcp-creds\n        path: op://v/i/f\n");
        assert_eq!(
            parse_err(&data),
            "file secret 'gcp-creds' must use a valid environment variable name"
        );
    }

    #[test]
    fn parse_fails_when_env_secret_name_is_not_a_variable_name() {
        let data = with_secrets(
            "      - kind: env\n        name: \"A;rm -rf ~\"\n        path: op://v/i/f\n",
        );
        assert_eq!(
            parse_err(&data),
            "env secret 'A;rm -rf ~' must use a valid environment variable name"
        );
    }

    #[test]
    fn parse_accepts_any_ssh_key_name() {
        let data = with_secrets(
            "      - kind: ssh\n        name: github-work\n        path: op://v/i/f\n",
        );
        assert!(Config::parse(&data).is_ok());
    }

    #[test]
    fn parse_fails_when_secret_name_is_duplicated() {
        let secret = "      - kind: env\n        name: A\n        path: op://v/i/f\n";
        let data = with_secrets(&secret.repeat(2));
        assert_eq!(
            parse_err(&data),
            "secret 'A' is defined more than once in account 'p'"
        );
    }

    #[test]
    fn parse_fails_on_malformed_yaml() {
        assert!(Config::parse("version: [1\n").is_err());
    }

    #[test]
    fn read_from_file_fails_when_file_is_missing() {
        let err = Config::read_from_file(Path::new("/nonexistent/config.yml")).unwrap_err();
        assert_eq!(
            err.to_string(),
            "failed to read config file /nonexistent/config.yml"
        );
    }

    #[test]
    fn profile_fails_with_available_profiles() {
        let config = Config::parse(VALID).unwrap();
        assert_eq!(
            config.profile("staging").unwrap_err().to_string(),
            "profile 'staging' not found in config (available profiles: personal, work)"
        );
    }

    #[test]
    fn secret_fails_with_available_secrets() {
        let config = Config::parse(VALID).unwrap();
        let account = config.profile("personal").unwrap();
        assert_eq!(account.secret("my-key").unwrap().kind, SecretKind::Ssh);
        assert_eq!(
            account.secret("NOPE").unwrap_err().to_string(),
            "secret 'NOPE' not found in profile 'personal' (available secrets: GITHUB_TOKEN, my-key, GOOGLE_APPLICATION_CREDENTIALS)"
        );
    }
}
