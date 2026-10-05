use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::path::Path;

const CONFIG: &str = "version: 1
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
  - name: work
    account: team.1password.com
";

/// Returns a temporary directory holding `config.yml` and an empty `cache` directory.
fn workspace(config: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.yml"), config).unwrap();
    std::fs::create_dir(dir.path().join("cache")).unwrap();
    dir
}

/// Returns a zsh-op command that uses the config and cache of `dir`.
fn zsh_op(dir: &Path) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!();
    cmd.env_remove("ZSH_OP_DEFAULT_PROFILE")
        .env("ZSH_OP_CONFIG_FILE", dir.join("config.yml"))
        .env("ZSH_OP_CACHE_DIR", dir.join("cache"));
    cmd
}

#[test]
fn help_lists_subcommands() {
    cargo_bin_cmd!().arg("--help").assert().success().stdout(
        predicate::str::contains("inspect")
            .and(predicate::str::contains("shell"))
            .and(predicate::str::contains("secret"))
            .and(predicate::str::contains("export"))
            .and(predicate::str::contains("exec")),
    );
}

#[test]
fn list_profiles() {
    let dir = workspace(CONFIG);
    zsh_op(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout("personal\nwork\n");
}

#[test]
fn list_secrets_with_config_flag() {
    let dir = workspace(CONFIG);
    cargo_bin_cmd!()
        .args(["list", "--profile", "personal", "--config"])
        .arg(dir.path().join("config.yml"))
        .assert()
        .success()
        .stdout("GITHUB_TOKEN\nmy-key\n");
}

#[test]
fn inspect_shows_profiles() {
    let dir = workspace(CONFIG);
    zsh_op(dir.path())
        .args(["inspect", "-p", "work"])
        .assert()
        .success()
        .stdout("Profile: work\n  Account: team.1password.com\n  Loaded: no\n");
}

#[test]
fn inspect_fails_when_config_missing() {
    let dir = tempfile::tempdir().unwrap();
    zsh_op(dir.path())
        .arg("inspect")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "zsh-op: error: failed to read config file",
        ));
}

#[test]
fn inspect_fails_on_invalid_config() {
    let dir = workspace(&CONFIG.replace("kind: env", "kind: token"));
    zsh_op(dir.path())
        .arg("inspect")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "secret 'GITHUB_TOKEN' has invalid kind: token (valid kinds: env, ssh, file)",
        ));
}

#[test]
fn export_cached_writes_nothing_before_any_profile_is_loaded() {
    let dir = workspace(CONFIG);
    zsh_op(dir.path())
        .args(["export", "--all", "--cached", "--format", "zsh"])
        .assert()
        .success()
        .stdout("");
}

#[test]
fn export_rejects_cached_with_refresh() {
    let dir = workspace(CONFIG);
    zsh_op(dir.path())
        .args(["export", "--cached", "--refresh"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn secret_fails_for_unknown_profile() {
    let dir = workspace(CONFIG);
    zsh_op(dir.path())
        .args(["secret", "-p", "staging", "GITHUB_TOKEN"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "profile 'staging' not found in config (available profiles: personal, work)",
        ));
}

#[test]
fn shell_uses_default_profile_from_environment() {
    let dir = workspace(CONFIG);
    zsh_op(dir.path())
        .env("ZSH_OP_DEFAULT_PROFILE", "staging")
        .arg("shell")
        .assert()
        .failure()
        .stderr(predicate::str::contains("profile 'staging' not found"));
}

#[test]
fn exec_requires_a_command() {
    let dir = workspace(CONFIG);
    zsh_op(dir.path()).arg("exec").assert().failure();
}
