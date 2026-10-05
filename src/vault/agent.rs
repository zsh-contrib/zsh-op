use anyhow::{bail, Context, Result};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[cfg(test)]
use mockall::automock;

/// KeyAgent adds private keys to an SSH agent.
#[cfg_attr(test, automock)]
pub trait KeyAgent {
    /// Returns the SHA256 fingerprints of the keys held by the agent.
    fn fingerprints(&self) -> Result<Vec<String>>;
    /// Adds the OpenSSH private `key` to the agent for the given `lifetime` (e.g. 1h).
    /// Adding a key the agent already holds resets its lifetime.
    fn add(&self, key: &str, lifetime: &str) -> Result<()>;
}

/// Agent talks to the running ssh-agent through `ssh-add`.
#[derive(Default)]
pub struct Agent;

impl Agent {
    /// Creates a new ssh-agent client.
    pub fn new() -> Self {
        Self
    }
}

impl KeyAgent for Agent {
    fn fingerprints(&self) -> Result<Vec<String>> {
        let output = Command::new("ssh-add")
            .args(["-l", "-E", "sha256"])
            .stdin(Stdio::null())
            .output()
            .context("failed to run ssh-add; is OpenSSH installed?")?;

        // ssh-add -l exit codes:
        //   0 = agent running with keys
        //   1 = agent running without keys
        //   2 = agent not running
        match output.status.code() {
            Some(0) => Ok(String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.split_whitespace().nth(1))
                .map(str::to_string)
                .collect()),
            Some(1) => Ok(Vec::new()),
            _ => bail!("SSH agent is not running (start it with: eval $(ssh-agent))"),
        }
    }

    fn add(&self, key: &str, lifetime: &str) -> Result<()> {
        // The key is piped through stdin, so it never touches the disk.
        let mut child = Command::new("ssh-add")
            .args(["-q", "-t", lifetime, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to run ssh-add; is OpenSSH installed?")?;

        let mut stdin = child.stdin.take().expect("ssh-add stdin is piped");
        stdin.write_all(key.as_bytes())?;
        if !key.ends_with('\n') {
            stdin.write_all(b"\n")?;
        }
        drop(stdin);

        let output = child.wait_with_output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("failed to add SSH key to agent: {}", stderr.trim());
        }
        Ok(())
    }
}

/// Returns the SHA256 fingerprint of an OpenSSH private key, as printed by `ssh-add -l`.
pub fn fingerprint(key: &str) -> Result<String> {
    let key = ssh_key::PrivateKey::from_openssh(key).context("invalid OpenSSH private key")?;
    Ok(key.fingerprint(ssh_key::HashAlg::Sha256).to_string())
}

/// A throwaway ed25519 key, generated once per test run.
#[cfg(test)]
pub static TEST_KEY: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    let key =
        ssh_key::PrivateKey::random(&mut ssh_key::rand_core::OsRng, ssh_key::Algorithm::Ed25519)
            .unwrap();
    key.to_openssh(ssh_key::LineEnding::LF).unwrap().to_string()
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_matches_ssh_keygen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        std::fs::write(&path, TEST_KEY.as_str()).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))
            .unwrap();

        let output = Command::new("ssh-keygen")
            .args(["-l", "-E", "sha256", "-f"])
            .arg(&path)
            .output()
            .unwrap();
        let expected = String::from_utf8(output.stdout).unwrap();

        assert_eq!(
            Some(fingerprint(&TEST_KEY).unwrap().as_str()),
            expected.split_whitespace().nth(1)
        );
    }

    #[test]
    #[ignore = "talks to the ssh-agent at $SSH_AUTH_SOCK"]
    fn agent_adds_key_from_stdin() {
        let agent = Agent::new();
        agent.add(&TEST_KEY, "1m").unwrap();
        assert!(agent
            .fingerprints()
            .unwrap()
            .contains(&fingerprint(&TEST_KEY).unwrap()));
    }

    #[test]
    fn fingerprint_fails_on_invalid_key() {
        assert!(fingerprint("not a key").is_err());
    }
}
