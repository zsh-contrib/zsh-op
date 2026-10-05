use crate::app::args::*;
use crate::log::{info, warn};
use crate::vault::*;
use anyhow::{bail, Context, Result};
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::process::{Command, ExitStatus};

/// Fails when any secret failed to load. The warnings were already printed.
fn finish(failed: usize) -> Result<()> {
    if failed > 0 {
        bail!("failed to load {failed} secret(s)");
    }
    Ok(())
}

/// Quotes `value` for POSIX shells, so it survives `eval` unchanged.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Writes `variables` to `writer` in the given format.
fn write_variables(
    writer: &mut dyn Write,
    format: ExportFormat,
    variables: &[Variable],
) -> Result<()> {
    match format {
        ExportFormat::Zsh | ExportFormat::Bash => {
            for variable in variables {
                writeln!(writer, "export {}={}", variable.key, quote(&variable.value))?;
            }
        }
        ExportFormat::Json => {
            let environment: BTreeMap<&str, &str> = variables
                .iter()
                .map(|v| (v.key.as_str(), v.value.as_str()))
                .collect();
            writeln!(writer, "{}", serde_json::to_string(&environment)?)?;
        }
    }
    Ok(())
}

/// Inspect and display the configured profiles, their secrets and their cache state.
pub struct InspectCommand {
    /// Writer used to output the inspected profiles.
    pub writer: Box<dyn Write>,
    /// Cache used to tell which profiles were loaded.
    pub cache: Cache,
}

impl InspectCommand {
    /// Execute the InspectCommand with the provided arguments.
    pub fn execute(&mut self, args: &InspectCommandArgs) -> Result<()> {
        let config = Config::read_from_file(&args.parent.config)?;
        let accounts = match &args.profile {
            Some(profile) => vec![config.profile(profile)?],
            None => config.accounts.iter().collect(),
        };

        for (i, account) in accounts.iter().enumerate() {
            if i > 0 {
                writeln!(self.writer)?;
            }
            let loaded = self.cache.loaded(&account.name)?.is_some();
            writeln!(self.writer, "Profile: {}", account.name)?;
            writeln!(self.writer, "  Account: {}", account.account)?;
            writeln!(
                self.writer,
                "  Loaded: {}",
                if loaded { "yes" } else { "no" }
            )?;
            if !account.secrets.is_empty() {
                writeln!(self.writer, "  Secrets:")?;
                for secret in &account.secrets {
                    let kind = secret.kind.to_string();
                    writeln!(
                        self.writer,
                        "    {kind:<4} {} ({})",
                        secret.name, secret.path
                    )?;
                }
            }
        }

        Ok(())
    }
}

/// List profile names, or the secret names of a profile, one per line.
pub struct ListCommand {
    /// Writer used to output the names.
    pub writer: Box<dyn Write>,
}

impl ListCommand {
    /// Execute the ListCommand with the provided arguments.
    pub fn execute(&mut self, args: &ListCommandArgs) -> Result<()> {
        let config = Config::read_from_file(&args.parent.config)?;
        match &args.profile {
            Some(profile) => {
                for secret in &config.profile(profile)?.secrets {
                    writeln!(self.writer, "{}", secret.name)?;
                }
            }
            None => {
                for account in &config.accounts {
                    writeln!(self.writer, "{}", account.name)?;
                }
            }
        }

        Ok(())
    }
}

/// Set up the shell environment with all secrets from a profile.
pub struct ShellCommand {
    /// Writer used to output the export statements.
    pub writer: Box<dyn Write>,
    /// Loader used to resolve the secrets.
    pub loader: Loader,
    /// Agent the SSH keys are added to.
    pub agent: Box<dyn KeyAgent>,
}

impl ShellCommand {
    /// Execute the ShellCommand with the provided arguments.
    pub fn execute(&mut self, args: &ShellCommandArgs) -> Result<()> {
        let config = Config::read_from_file(&args.parent.config)?;
        let account = config.profile(&args.profile)?;
        let runtime = RuntimeDir::new(args.output.runtime_dir.clone());

        // Export the environment and file secrets
        let secrets = account.secrets.iter().filter(|s| s.is_variable());
        let (variables, mut failed) =
            self.loader
                .resolve(&runtime, account, secrets, Source::Any(args.refresh));
        write_variables(&mut self.writer, args.output.export_format(), &variables)?;
        if !variables.is_empty() {
            info(format!(
                "loaded {} environment and file secret(s)",
                variables.len()
            ));
        }

        // Add the SSH keys to the agent
        let keys: Vec<&Secret> = account
            .secrets
            .iter()
            .filter(|s| s.kind == SecretKind::Ssh)
            .collect();
        if !keys.is_empty() {
            match self.agent.fingerprints() {
                Ok(present) => {
                    let mut loaded = 0;
                    for key in &keys {
                        match self.loader.add_key(
                            self.agent.as_ref(),
                            &present,
                            account,
                            key,
                            &args.expiration,
                            args.refresh,
                        ) {
                            Ok(_) => loaded += 1,
                            Err(err) => {
                                warn(format!("failed to load SSH key '{}': {err:#}", key.name));
                                failed += 1;
                            }
                        }
                    }
                    if loaded > 0 {
                        info(format!(
                            "loaded {loaded} SSH key(s) with {} expiration",
                            args.expiration
                        ));
                    }
                }
                Err(err) => {
                    warn(format!("failed to load {} SSH key(s): {err:#}", keys.len()));
                    failed += keys.len();
                }
            }
        }

        // Record the profile so its cached secrets are exported on shell startup
        self.loader.cache.save(account)?;
        finish(failed)
    }
}

/// Load an individual secret on demand.
pub struct SecretCommand {
    /// Writer used to output the secret value, path or export statement.
    pub writer: Box<dyn Write>,
    /// Loader used to resolve the secret.
    pub loader: Loader,
    /// Agent SSH keys are added to.
    pub agent: Box<dyn KeyAgent>,
}

impl SecretCommand {
    /// Execute the SecretCommand with the provided arguments.
    pub fn execute(&mut self, args: &SecretCommandArgs) -> Result<()> {
        let config = Config::read_from_file(&args.parent.config)?;
        let account = config.profile(&args.profile)?;
        let secret = account.secret(&args.name)?;

        if secret.kind == SecretKind::Ssh {
            if args.export {
                warn("--export is not valid for SSH keys (ignored)");
            }
            let present = self.agent.fingerprints()?;
            let added = self.loader.add_key(
                self.agent.as_ref(),
                &present,
                account,
                secret,
                &args.expiration,
                args.refresh,
            )?;
            if added {
                info(format!(
                    "SSH key '{}' loaded with {} expiration",
                    secret.name, args.expiration
                ));
            } else {
                info(format!(
                    "SSH key '{}' is already in the agent (use --refresh to reset its expiration)",
                    secret.name
                ));
            }
            return Ok(());
        }

        let runtime = RuntimeDir::new(args.output.runtime_dir.clone());
        let (variables, failed) =
            self.loader
                .resolve(&runtime, account, [secret], Source::Any(args.refresh));
        finish(failed)?;

        if args.export {
            write_variables(&mut self.writer, args.output.export_format(), &variables)?;
        } else {
            for variable in &variables {
                writeln!(self.writer, "{}", variable.value)?;
            }
        }
        Ok(())
    }
}

/// Export the environment and file secrets of a profile as shell statements.
pub struct ExportCommand {
    /// Writer used to output the export statements.
    pub writer: Box<dyn Write>,
    /// Loader used to resolve the secrets.
    pub loader: Loader,
}

impl ExportCommand {
    /// Execute the ExportCommand with the provided arguments.
    pub fn execute(&mut self, args: &ExportCommandArgs) -> Result<()> {
        let config = Config::read_from_file(&args.parent.config)?;
        let accounts = if args.all {
            config.accounts.iter().collect()
        } else {
            vec![config.profile(&args.profile)?]
        };
        let runtime = RuntimeDir::new(args.output.runtime_dir.clone());

        let mut variables = Vec::new();
        let mut failed = 0;
        for account in accounts {
            let (mut resolved, count) = if args.cached {
                // Only profiles loaded before are exported, and only the secrets that are
                // still configured. The kind always comes from the current config.
                let Some(names) = self.loader.cache.loaded(&account.name)? else {
                    continue;
                };
                let secrets = account
                    .secrets
                    .iter()
                    .filter(|s| s.is_variable())
                    .filter(|s| names.contains(&s.name));
                self.loader
                    .resolve(&runtime, account, secrets, Source::Cache)
            } else {
                let secrets = account.secrets.iter().filter(|s| s.is_variable());
                let resolved =
                    self.loader
                        .resolve(&runtime, account, secrets, Source::Any(args.refresh));
                self.loader.cache.save(account)?;
                resolved
            };
            variables.append(&mut resolved);
            failed += count;
        }

        write_variables(&mut self.writer, args.output.export_format(), &variables)?;
        finish(failed)
    }
}

/// Execute a command with the secrets of a profile in its environment.
pub struct ExecCommand {
    /// Loader used to resolve the secrets.
    pub loader: Loader,
}

impl ExecCommand {
    /// Execute the ExecCommand with the provided arguments.
    pub fn execute(&mut self, args: &ExecCommandArgs) -> Result<ExitStatus> {
        let config = Config::read_from_file(&args.parent.config)?;
        let account = config.profile(&args.profile)?;

        // File secrets live only as long as the command
        let dir = tempfile::Builder::new()
            .prefix("zsh-op.")
            .tempdir()
            .context("failed to create file secret runtime directory")?;
        let runtime = RuntimeDir::new(Some(dir.path().to_path_buf()));

        let secrets = account.secrets.iter().filter(|s| s.is_variable());
        let (variables, failed) =
            self.loader
                .resolve(&runtime, account, secrets, Source::Any(args.refresh));
        // Do not run the command with a partial environment
        finish(failed)?;

        // Prepare the command
        let mut arguments = VecDeque::from(args.command.clone());
        let name = match arguments.pop_front() {
            Some(value) => value,
            None => String::from("sh"),
        };

        // Execute the command
        let mut child = Command::new(&name)
            .args(arguments)
            .envs(variables.iter().map(|v| (&v.key, &v.value)))
            .spawn()
            .with_context(|| format!("failed to execute {name}"))?;

        // Like system(3): the terminal delivers Ctrl-C and Ctrl-\ to the whole process
        // group, so let the command handle them while we wait to clean up the file secrets.
        let _guard = SignalGuard::ignore(&[libc::SIGINT, libc::SIGQUIT]);
        let status = child.wait()?;
        Ok(status)
    }
}

/// SignalGuard ignores signals until it is dropped, then restores their previous handlers.
struct SignalGuard(Vec<(libc::c_int, libc::sighandler_t)>);

impl SignalGuard {
    fn ignore(signals: &[libc::c_int]) -> Self {
        Self(
            signals
                .iter()
                // SAFETY: SIG_IGN is a valid disposition and no Rust handler is installed.
                .map(|&signal| (signal, unsafe { libc::signal(signal, libc::SIG_IGN) }))
                .collect(),
        )
    }
}

impl Drop for SignalGuard {
    fn drop(&mut self) {
        for &(signal, handler) in &self.0 {
            // SAFETY: restores the disposition returned by signal(2) above.
            unsafe { libc::signal(signal, handler) };
        }
    }
}

/// Clear the cached secrets of a profile.
pub struct ClearCommand {
    /// Cache the secrets are deleted from.
    pub cache: Cache,
}

impl ClearCommand {
    /// Execute the ClearCommand with the provided arguments.
    pub fn execute(&mut self, args: &ClearCommandArgs) -> Result<()> {
        let config = Config::read_from_file(&args.parent.config)?;
        let account = config.profile(&args.profile)?;

        let count = self.cache.clear(account)?;
        info(format!(
            "cleared {count} cached secret(s) for profile '{}'",
            account.name
        ));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use std::path::{Path, PathBuf};
    use std::sync::*;

    #[derive(Clone)]
    struct Writer(Arc<Mutex<Vec<u8>>>);

    impl Writer {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }

        fn contents(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    const CONFIG: &str = indoc! {"
        version: 1
        accounts:
          - name: personal
            account: my.1password.com
            secrets:
              - kind: env
                name: GITHUB_TOKEN
                path: op://Personal/GitHub/token
              - kind: file
                name: GCP_CREDENTIALS
                path: op://Personal/GCP/credentials
              - kind: ssh
                name: my-key
                path: op://Private/SSH/private key?ssh-format=openssh
          - name: work
            account: team.1password.com
            secrets:
              - kind: env
                name: API_KEY
                path: op://Infra/Prod/API_KEY
    "};

    /// Fixture holds a temporary config, cache directory, runtime directory and store.
    struct Fixture {
        dir: tempfile::TempDir,
        store: MemoryStore,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("config.yml"), CONFIG).unwrap();
            Self {
                dir,
                store: MemoryStore::default(),
            }
        }

        /// Caches `value` as secret `name` of `profile`.
        fn cached(self, profile: &str, name: &str, value: &str) -> Self {
            self.store
                .set(&Cache::service(profile), name, value)
                .unwrap();
            self
        }

        /// Records `profile` as loaded with the given metadata lines.
        fn loaded(self, profile: &str, metadata: &str) -> Self {
            let dir = self.dir.path().join("cache");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("{profile}.metadata")), metadata).unwrap();
            self
        }

        fn parent(&self) -> ProgramArgs {
            ProgramArgs {
                config: self.dir.path().join("config.yml"),
                cache_dir: self.dir.path().join("cache"),
            }
        }

        fn output(&self, format: ExportFormat) -> OutputArgs {
            OutputArgs {
                format: Some(format),
                runtime_dir: Some(self.runtime_dir()),
            }
        }

        fn runtime_dir(&self) -> PathBuf {
            self.dir.path().join("runtime")
        }

        fn file(&self, profile: &str, name: &str) -> PathBuf {
            self.runtime_dir().join("files").join(profile).join(name)
        }

        fn cache(&self) -> Cache {
            Cache::new(Box::new(self.store.clone()), &self.dir.path().join("cache"))
        }

        fn loader(&self, client: MockSecretClient) -> Loader {
            Loader {
                client: Box::new(client),
                cache: self.cache(),
            }
        }

        fn value(&self, profile: &str, name: &str) -> Option<String> {
            self.store.value(&Cache::service(profile), name)
        }
    }

    fn offline() -> MockSecretClient {
        let mut client = MockSecretClient::new();
        client.expect_read().never();
        client
    }

    fn expect_read(client: &mut MockSecretClient, path: &'static str, value: &'static str) {
        client
            .expect_read()
            .withf(move |_, p| p == path)
            .times(1)
            .returning(move |_, _| Ok(value.to_string()));
    }

    fn agent(present: Vec<String>) -> MockKeyAgent {
        let mut agent = MockKeyAgent::new();
        agent.expect_fingerprints().return_once(move || Ok(present));
        agent
    }

    fn shell_args(fixture: &Fixture, refresh: bool) -> ShellCommandArgs {
        ShellCommandArgs {
            parent: fixture.parent(),
            profile: "personal".into(),
            expiration: "1h".into(),
            refresh,
            output: fixture.output(ExportFormat::Zsh),
        }
    }

    fn secret_args(fixture: &Fixture, name: &str, export: bool) -> SecretCommandArgs {
        SecretCommandArgs {
            parent: fixture.parent(),
            name: name.into(),
            profile: "personal".into(),
            export,
            expiration: "8h".into(),
            refresh: false,
            output: fixture.output(ExportFormat::Zsh),
        }
    }

    fn export_args(fixture: &Fixture, all: bool, cached: bool) -> ExportCommandArgs {
        ExportCommandArgs {
            parent: fixture.parent(),
            profile: "personal".into(),
            all,
            cached,
            refresh: false,
            output: fixture.output(ExportFormat::Zsh),
        }
    }

    #[test]
    fn quote_wraps_value_in_single_quotes() {
        assert_eq!(quote("brown fox"), "'brown fox'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn quoted_values_survive_shell_eval() {
        let values = [
            "plain",
            "it's \"quoted\"",
            "multi\nline\n",
            "$(touch /tmp/zsh-op-pwned) `id` $HOME \\n",
            "unicode ✓ 🔑",
        ];
        for value in values {
            let mut out = Vec::new();
            let vars = [Variable {
                key: "VALUE".into(),
                value: value.into(),
            }];
            write_variables(&mut out, ExportFormat::Zsh, &vars).unwrap();

            let output = Command::new("sh")
                .args(["-c", r#"eval "$1"; printf %s "$VALUE""#, "sh"])
                .arg(String::from_utf8(out).unwrap())
                .output()
                .unwrap();
            assert_eq!(String::from_utf8(output.stdout).unwrap(), value);
        }
    }

    #[test]
    fn write_variables_writes_json_object() {
        let mut out = Vec::new();
        let vars = [
            Variable {
                key: "B".into(),
                value: "2".into(),
            },
            Variable {
                key: "A".into(),
                value: "line\n".into(),
            },
        ];
        write_variables(&mut out, ExportFormat::Json, &vars).unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "{\"A\":\"line\\n\",\"B\":\"2\"}\n"
        );
    }

    #[test]
    fn inspect_writes_profiles() -> Result<()> {
        let fixture = Fixture::new().loaded("personal", "env:GITHUB_TOKEN\n");
        let writer = Writer::new();
        let mut cmd = InspectCommand {
            writer: Box::new(writer.clone()),
            cache: fixture.cache(),
        };

        cmd.execute(&InspectCommandArgs {
            parent: fixture.parent(),
            profile: None,
        })?;

        let expected = indoc! {"
            Profile: personal
              Account: my.1password.com
              Loaded: yes
              Secrets:
                env  GITHUB_TOKEN (op://Personal/GitHub/token)
                file GCP_CREDENTIALS (op://Personal/GCP/credentials)
                ssh  my-key (op://Private/SSH/private key?ssh-format=openssh)

            Profile: work
              Account: team.1password.com
              Loaded: no
              Secrets:
                env  API_KEY (op://Infra/Prod/API_KEY)
        "};
        assert_eq!(writer.contents(), expected);
        Ok(())
    }

    #[test]
    fn inspect_fails_for_unknown_profile() {
        let fixture = Fixture::new();
        let mut cmd = InspectCommand {
            writer: Box::new(Writer::new()),
            cache: fixture.cache(),
        };

        let result = cmd.execute(&InspectCommandArgs {
            parent: fixture.parent(),
            profile: Some("staging".into()),
        });

        assert_eq!(
            result.unwrap_err().to_string(),
            "profile 'staging' not found in config (available profiles: personal, work)"
        );
    }

    #[test]
    fn list_writes_profile_names() -> Result<()> {
        let fixture = Fixture::new();
        let writer = Writer::new();
        let mut cmd = ListCommand {
            writer: Box::new(writer.clone()),
        };

        cmd.execute(&ListCommandArgs {
            parent: fixture.parent(),
            profile: None,
        })?;

        assert_eq!(writer.contents(), "personal\nwork\n");
        Ok(())
    }

    #[test]
    fn list_writes_secret_names_of_profile() -> Result<()> {
        let fixture = Fixture::new();
        let writer = Writer::new();
        let mut cmd = ListCommand {
            writer: Box::new(writer.clone()),
        };

        cmd.execute(&ListCommandArgs {
            parent: fixture.parent(),
            profile: Some("personal".into()),
        })?;

        assert_eq!(writer.contents(), "GITHUB_TOKEN\nGCP_CREDENTIALS\nmy-key\n");
        Ok(())
    }

    #[test]
    fn shell_loads_cached_secrets_without_contacting_1password() -> Result<()> {
        let fixture = Fixture::new()
            .cached("personal", "GITHUB_TOKEN", "brown-fox")
            .cached("personal", "GCP_CREDENTIALS", "{\"type\":\"sa\"}")
            .cached("personal", "my-key", &TEST_KEY);
        let mut agent = agent(vec![]);
        agent
            .expect_add()
            .withf(|key, lifetime| *key == *TEST_KEY && lifetime == "1h")
            .times(1)
            .returning(|_, _| Ok(()));
        let writer = Writer::new();
        let mut cmd = ShellCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(offline()),
            agent: Box::new(agent),
        };

        cmd.execute(&shell_args(&fixture, false))?;

        let file = fixture.file("personal", "GCP_CREDENTIALS");
        assert_eq!(
            writer.contents(),
            format!(
                "export GITHUB_TOKEN='brown-fox'\nexport GCP_CREDENTIALS='{}'\n",
                file.display()
            )
        );
        assert_eq!(std::fs::read_to_string(file)?, "{\"type\":\"sa\"}");
        assert_eq!(
            fixture.cache().loaded("personal")?,
            Some(vec![
                "GITHUB_TOKEN".into(),
                "GCP_CREDENTIALS".into(),
                "my-key".into()
            ])
        );
        Ok(())
    }

    #[test]
    fn shell_fetches_uncached_secrets_and_caches_them() -> Result<()> {
        let fixture = Fixture::new();
        let mut client = MockSecretClient::new();
        expect_read(&mut client, "op://Personal/GitHub/token", "brown-fox");
        expect_read(&mut client, "op://Personal/GCP/credentials", "{}");
        expect_read(
            &mut client,
            "op://Private/SSH/private key?ssh-format=openssh",
            TEST_KEY.as_str(),
        );
        let mut agent = agent(vec![]);
        agent.expect_add().times(1).returning(|_, _| Ok(()));
        let mut cmd = ShellCommand {
            writer: Box::new(Writer::new()),
            loader: fixture.loader(client),
            agent: Box::new(agent),
        };

        cmd.execute(&shell_args(&fixture, false))?;

        assert_eq!(
            fixture.value("personal", "GITHUB_TOKEN").as_deref(),
            Some("brown-fox")
        );
        assert_eq!(
            fixture.value("personal", "GCP_CREDENTIALS").as_deref(),
            Some("{}")
        );
        assert_eq!(
            fixture.value("personal", "my-key").as_deref(),
            Some(TEST_KEY.as_str())
        );
        Ok(())
    }

    #[test]
    fn shell_refresh_bypasses_cache_and_readds_keys() -> Result<()> {
        let fixture = Fixture::new()
            .cached("personal", "GITHUB_TOKEN", "stale")
            .cached("personal", "GCP_CREDENTIALS", "stale")
            .cached("personal", "my-key", &TEST_KEY);
        let mut client = MockSecretClient::new();
        expect_read(&mut client, "op://Personal/GitHub/token", "fresh");
        expect_read(&mut client, "op://Personal/GCP/credentials", "fresh");
        expect_read(
            &mut client,
            "op://Private/SSH/private key?ssh-format=openssh",
            TEST_KEY.as_str(),
        );
        let mut agent = agent(vec![fingerprint(&TEST_KEY)?]);
        agent.expect_add().times(1).returning(|_, _| Ok(()));
        let mut cmd = ShellCommand {
            writer: Box::new(Writer::new()),
            loader: fixture.loader(client),
            agent: Box::new(agent),
        };

        cmd.execute(&shell_args(&fixture, true))?;

        assert_eq!(
            fixture.value("personal", "GITHUB_TOKEN").as_deref(),
            Some("fresh")
        );
        Ok(())
    }

    #[test]
    fn shell_skips_keys_already_in_agent() -> Result<()> {
        let fixture = Fixture::new()
            .cached("personal", "GITHUB_TOKEN", "a")
            .cached("personal", "GCP_CREDENTIALS", "b")
            .cached("personal", "my-key", &TEST_KEY);
        let mut agent = agent(vec![fingerprint(&TEST_KEY)?]);
        agent.expect_add().never();
        let mut cmd = ShellCommand {
            writer: Box::new(Writer::new()),
            loader: fixture.loader(offline()),
            agent: Box::new(agent),
        };

        cmd.execute(&shell_args(&fixture, false))
    }

    #[test]
    fn shell_continues_past_failing_secrets() -> Result<()> {
        let fixture = Fixture::new().cached("personal", "GCP_CREDENTIALS", "{}");
        let mut client = MockSecretClient::new();
        client
            .expect_read()
            .withf(|_, p| p == "op://Personal/GitHub/token")
            .returning(|_, _| Err(anyhow::anyhow!("oh no")));
        let mut agent = MockKeyAgent::new();
        agent
            .expect_fingerprints()
            .returning(|| Err(anyhow::anyhow!("SSH agent is not running")));
        let writer = Writer::new();
        let mut cmd = ShellCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(client),
            agent: Box::new(agent),
        };

        let result = cmd.execute(&shell_args(&fixture, false));

        assert_eq!(
            result.unwrap_err().to_string(),
            "failed to load 2 secret(s)"
        );
        assert!(writer.contents().starts_with("export GCP_CREDENTIALS="));
        assert!(fixture.cache().loaded("personal")?.is_some());
        Ok(())
    }

    #[test]
    fn secret_prints_env_value() -> Result<()> {
        let fixture = Fixture::new().cached("personal", "GITHUB_TOKEN", "brown-fox");
        let writer = Writer::new();
        let mut cmd = SecretCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(offline()),
            agent: Box::new(MockKeyAgent::new()),
        };

        cmd.execute(&secret_args(&fixture, "GITHUB_TOKEN", false))?;

        assert_eq!(writer.contents(), "brown-fox\n");
        Ok(())
    }

    #[test]
    fn secret_exports_env_value() -> Result<()> {
        let fixture = Fixture::new().cached("personal", "GITHUB_TOKEN", "brown-fox");
        let writer = Writer::new();
        let mut cmd = SecretCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(offline()),
            agent: Box::new(MockKeyAgent::new()),
        };

        cmd.execute(&secret_args(&fixture, "GITHUB_TOKEN", true))?;

        assert_eq!(writer.contents(), "export GITHUB_TOKEN='brown-fox'\n");
        Ok(())
    }

    #[test]
    fn secret_writes_file_and_prints_path() -> Result<()> {
        let fixture = Fixture::new();
        let mut client = MockSecretClient::new();
        expect_read(&mut client, "op://Personal/GCP/credentials", "{}");
        let writer = Writer::new();
        let mut cmd = SecretCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(client),
            agent: Box::new(MockKeyAgent::new()),
        };

        cmd.execute(&secret_args(&fixture, "GCP_CREDENTIALS", false))?;

        let file = fixture.file("personal", "GCP_CREDENTIALS");
        assert_eq!(writer.contents(), format!("{}\n", file.display()));
        assert_eq!(std::fs::read_to_string(file)?, "{}");
        Ok(())
    }

    #[test]
    fn secret_adds_ssh_key_with_expiration() -> Result<()> {
        let fixture = Fixture::new().cached("personal", "my-key", &TEST_KEY);
        let mut agent = agent(vec![]);
        agent
            .expect_add()
            .withf(|_, lifetime| lifetime == "8h")
            .times(1)
            .returning(|_, _| Ok(()));
        let writer = Writer::new();
        let mut cmd = SecretCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(offline()),
            agent: Box::new(agent),
        };

        cmd.execute(&secret_args(&fixture, "my-key", true))?;

        assert_eq!(writer.contents(), "");
        Ok(())
    }

    #[test]
    fn secret_fails_for_unknown_secret() {
        let fixture = Fixture::new();
        let mut cmd = SecretCommand {
            writer: Box::new(Writer::new()),
            loader: fixture.loader(offline()),
            agent: Box::new(MockKeyAgent::new()),
        };

        let result = cmd.execute(&secret_args(&fixture, "NOPE", false));

        assert!(result
            .unwrap_err()
            .to_string()
            .starts_with("secret 'NOPE' not found in profile 'personal'"));
    }

    #[test]
    fn export_cached_exports_only_loaded_profiles() -> Result<()> {
        let fixture = Fixture::new()
            .cached("personal", "GITHUB_TOKEN", "brown-fox")
            .cached("work", "API_KEY", "not-loaded")
            .loaded(
                "personal",
                "env:GITHUB_TOKEN\nfile:GCP_CREDENTIALS\nssh:my-key\n",
            );
        let writer = Writer::new();
        let mut cmd = ExportCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(offline()),
        };

        cmd.execute(&export_args(&fixture, true, true))?;

        // GCP_CREDENTIALS is recorded but not cached, so it is skipped silently.
        assert_eq!(writer.contents(), "export GITHUB_TOKEN='brown-fox'\n");
        Ok(())
    }

    #[test]
    fn export_cached_uses_kind_from_current_config() -> Result<()> {
        // The metadata still says env, but the config now declares a file secret.
        let fixture = Fixture::new()
            .cached("personal", "GCP_CREDENTIALS", "{}")
            .loaded("personal", "env:GCP_CREDENTIALS\n");
        let writer = Writer::new();
        let mut cmd = ExportCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(offline()),
        };

        cmd.execute(&export_args(&fixture, false, true))?;

        let file = fixture.file("personal", "GCP_CREDENTIALS");
        assert_eq!(
            writer.contents(),
            format!("export GCP_CREDENTIALS='{}'\n", file.display())
        );
        assert_eq!(std::fs::read_to_string(file)?, "{}");
        Ok(())
    }

    #[test]
    fn export_cached_writes_nothing_without_loaded_profiles() -> Result<()> {
        let fixture = Fixture::new().cached("personal", "GITHUB_TOKEN", "brown-fox");
        let writer = Writer::new();
        let mut cmd = ExportCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(offline()),
        };

        cmd.execute(&export_args(&fixture, true, true))?;

        assert_eq!(writer.contents(), "");
        Ok(())
    }

    #[test]
    fn export_fetches_profile_and_records_it() -> Result<()> {
        let fixture = Fixture::new();
        let mut client = MockSecretClient::new();
        expect_read(&mut client, "op://Infra/Prod/API_KEY", "it's");
        let writer = Writer::new();
        let mut cmd = ExportCommand {
            writer: Box::new(writer.clone()),
            loader: fixture.loader(client),
        };

        cmd.execute(&ExportCommandArgs {
            profile: "work".into(),
            output: fixture.output(ExportFormat::Json),
            ..export_args(&fixture, false, false)
        })?;

        assert_eq!(writer.contents(), "{\"API_KEY\":\"it's\"}\n");
        assert_eq!(
            fixture.cache().loaded("work")?,
            Some(vec!["API_KEY".into()])
        );
        Ok(())
    }

    #[test]
    fn exec_runs_command_with_secrets() -> Result<()> {
        let fixture = Fixture::new()
            .cached("personal", "GITHUB_TOKEN", "brown-fox")
            .cached("personal", "GCP_CREDENTIALS", "{}");
        let mut cmd = ExecCommand {
            loader: fixture.loader(offline()),
        };

        let status = cmd.execute(&ExecCommandArgs {
            parent: fixture.parent(),
            profile: "personal".into(),
            refresh: false,
            command: vec![
                "sh".into(),
                "-c".into(),
                r#"[ "$GITHUB_TOKEN" = "brown-fox" ] && [ "$(cat "$GCP_CREDENTIALS")" = "{}" ] && printf %s "$GCP_CREDENTIALS" > "$0""#.into(),
                fixture.dir.path().join("path").to_string_lossy().into_owned(),
            ],
        })?;

        assert!(status.success());
        // The file secret is removed once the command exits.
        let file = std::fs::read_to_string(fixture.dir.path().join("path"))?;
        assert!(!Path::new(&file).exists());
        Ok(())
    }

    #[test]
    fn exec_returns_command_exit_status() -> Result<()> {
        let fixture = Fixture::new()
            .cached("personal", "GITHUB_TOKEN", "a")
            .cached("personal", "GCP_CREDENTIALS", "b");
        let mut cmd = ExecCommand {
            loader: fixture.loader(offline()),
        };

        let status = cmd.execute(&ExecCommandArgs {
            parent: fixture.parent(),
            profile: "personal".into(),
            refresh: false,
            command: vec!["sh".into(), "-c".into(), "exit 3".into()],
        })?;

        assert_eq!(status.code(), Some(3));
        Ok(())
    }

    #[test]
    fn exec_does_not_run_command_with_partial_environment() {
        let fixture = Fixture::new().cached("personal", "GITHUB_TOKEN", "a");
        let mut client = MockSecretClient::new();
        client
            .expect_read()
            .returning(|_, _| Err(anyhow::anyhow!("oh no")));
        let mut cmd = ExecCommand {
            loader: fixture.loader(client),
        };
        let marker = fixture.dir.path().join("ran");

        let result = cmd.execute(&ExecCommandArgs {
            parent: fixture.parent(),
            profile: "personal".into(),
            refresh: false,
            command: vec!["touch".into(), marker.to_string_lossy().into_owned()],
        });

        assert_eq!(
            result.unwrap_err().to_string(),
            "failed to load 1 secret(s)"
        );
        assert!(!marker.exists());
    }

    #[test]
    fn clear_deletes_cached_secrets() -> Result<()> {
        let fixture = Fixture::new()
            .cached("personal", "GITHUB_TOKEN", "a")
            .cached("work", "API_KEY", "b")
            .loaded("personal", "env:GITHUB_TOKEN\n");
        let mut cmd = ClearCommand {
            cache: fixture.cache(),
        };

        cmd.execute(&ClearCommandArgs {
            parent: fixture.parent(),
            profile: "personal".into(),
        })?;

        assert_eq!(fixture.value("personal", "GITHUB_TOKEN"), None);
        assert_eq!(fixture.value("work", "API_KEY").as_deref(), Some("b"));
        assert_eq!(fixture.cache().loaded("personal")?, None);
        Ok(())
    }
}
