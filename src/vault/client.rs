use anyhow::{bail, Context, Result};
use std::{
    cell::RefCell,
    collections::HashMap,
    process::{Command, Stdio},
};

#[cfg(test)]
use mockall::automock;

/// SecretClient reads secrets from 1Password.
#[cfg_attr(test, automock)]
pub trait SecretClient {
    /// Reads the secret referenced by `path` from the 1Password `account`.
    fn read(&self, account: &str, path: &str) -> Result<String>;
}

/// Client reads secrets through the 1Password CLI (`op`).
#[derive(Default)]
pub struct Client {
    /// Sign-in state per account, checked once per run.
    signed_in: RefCell<HashMap<String, bool>>,
}

impl Client {
    /// Creates a new 1Password CLI client.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fails with a hint when the CLI is not signed in to `account`.
    fn ensure_signed_in(&self, account: &str) -> Result<()> {
        let mut cache = self.signed_in.borrow_mut();
        let signed_in = match cache.get(account) {
            Some(value) => *value,
            None => {
                let status = Command::new("op")
                    .args(["account", "get", "--account", account])
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .context("failed to run the 1Password CLI (op); is it installed?")?;
                *cache.entry(account.to_string()).or_insert(status.success())
            }
        };

        if !signed_in {
            bail!(
                "not signed in to 1Password account {account} (run: op signin --account {account})"
            );
        }
        Ok(())
    }
}

impl SecretClient for Client {
    fn read(&self, account: &str, path: &str) -> Result<String> {
        self.ensure_signed_in(account)?;

        // stdin and stderr stay attached to the terminal, so `op` can prompt
        // for authorization and report its own errors.
        let output = Command::new("op")
            .args(["read", "--no-newline", "--account", account, path])
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit())
            .output()
            .context("failed to run the 1Password CLI (op); is it installed?")?;
        if !output.status.success() {
            bail!("failed to read {path} from 1Password");
        }

        let value = String::from_utf8(output.stdout)
            .with_context(|| format!("secret {path} is not valid UTF-8"))?;
        if value.is_empty() {
            bail!("secret {path} is empty");
        }
        Ok(value)
    }
}
