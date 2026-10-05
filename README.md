# zsh-op

> 1Password CLI for Zsh — secure credential caching, multi-profile support, and SSH key management.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE) [![CI](https://github.com/zsh-contrib/zsh-op/actions/workflows/ci.yml/badge.svg)](https://github.com/zsh-contrib/zsh-op/actions/workflows/ci.yml)

Stop typing `op read` by hand. `zsh-op` reads a YAML config, fetches secrets from 1Password on first use, caches them in macOS Keychain, and exports them automatically on every shell start — with SSH keys loaded into ssh-agent and credentials ready before you run a single command.

The work is done by a small Rust binary, `zsh-op`; the zsh plugin wraps it with the `op-shell` and `op-secret` commands. Secrets never appear in process arguments and SSH keys are handed to ssh-agent without touching the disk.

![demo](docs/demo.webp)

## Requirements

- macOS (Keychain) or Linux (Secret Service)
- [1Password CLI](https://developer.1password.com/docs/cli/get-started/) (`op`)
- OpenSSH (`ssh-add`) for SSH keys

## Installation

### The `zsh-op` binary

The plugin needs the `zsh-op` binary on your `PATH` (or set `ZSH_OP_BIN` to its location).

**Nix:**

```bash
nix profile install github:zsh-contrib/zsh-op
```

**Cargo:**

```bash
cargo install --git https://github.com/zsh-contrib/zsh-op
```

**Prebuilt:** download `zsh-op-<system>` from the [latest release](https://github.com/zsh-contrib/zsh-op/releases/latest), or let zinit fetch it:

```zsh
zinit ice from"gh-r" as"program" mv"zsh-op-* -> zsh-op"
zinit light zsh-contrib/zsh-op
```

### The zsh plugin

### Using zinit

```zsh
zinit load zsh-contrib/zsh-op
```

### Using sheldon

```toml
[plugins.zsh-op]
github = "zsh-contrib/zsh-op"
```

### Manual

```zsh
git clone https://github.com/zsh-contrib/zsh-op.git ~/.zsh/plugins/zsh-op
source ~/.zsh/plugins/zsh-op/zsh-op.plugin.zsh
```

## Configuration

Create `~/.config/op/config.yml`:

```yaml
version: 1

accounts:
  - name: personal
    account: my.1password.com
    secrets:
      - kind: env
        name: GITHUB_TOKEN
        path: op://Personal/GitHub/Secrets/GITHUB_TOKEN

      - kind: ssh
        name: personal-key
        path: op://Private/SSH Key/private key?ssh-format=openssh

      - kind: file
        name: GOOGLE_APPLICATION_CREDENTIALS
        path: op://Personal/GCP/service-account-json

  - name: work
    account: team.1password.com
    secrets:
      - kind: env
        name: MYAPP_API_KEY
        path: op://Infra/Prod/API_KEY

      - kind: ssh
        name: github-work
        path: op://Employee/GitHub SSH/private key?ssh-format=openssh
```

See [config.example.yml](config.example.yml) for a complete annotated example. To find the correct `op://` path, right-click an item in the 1Password desktop app and select **Copy Secret Reference**. Append `?ssh-format=openssh` for SSH keys. File secrets are materialized in a private runtime directory, cleaned up when the shell exits, and the `name` is exported as an environment variable containing that file path.

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `ZSH_OP_BIN` | `zsh-op` | Path or name of the `zsh-op` binary |
| `ZSH_OP_CONFIG_FILE` | `~/.config/op/config.yml` | Config file location |
| `ZSH_OP_CACHE_DIR` | `~/.cache/op` | Cache directory |
| `ZSH_OP_AUTO_EXPORT` | `true` | Auto-export env vars on shell init |
| `ZSH_OP_DEFAULT_PROFILE` | `personal` | Default profile name |

## Usage

### `op-shell`

Set up your shell environment with all secrets from a profile. File secrets are written to disk and exported as paths.

```
Usage: op-shell [options] [profile]

Options:
  -e, --expiration TIME    SSH key expiration (default: 1h)
  -c, --config PATH        Config file path
  -r, --refresh            Force refresh from 1Password
  -h, --help               Show help
```

```bash
op-shell              # setup default profile
op-shell work         # setup work profile
op-shell work -e 8h   # setup with 8-hour SSH key expiration
op-shell -r personal  # force refresh from 1Password
```

### `op-secret`

Load an individual secret on-demand.

```
Usage: op-secret [options] <secret-name>

Options:
  -p, --profile PROFILE    Profile (default: personal)
  -x, --export             Export env secret to current shell
  -e, --expiration TIME    SSH key expiration (default: 1h)
  -r, --refresh            Force refresh from 1Password
  -c, --config PATH        Config file path
  -h, --help               Show help
```

```bash
op-secret GITHUB_TOKEN      # load and print a secret
op-secret GITHUB_TOKEN -x   # export to current shell
op-secret GOOGLE_APPLICATION_CREDENTIALS -x # write file and export path
op-secret github-work       # load an SSH key
op-secret -p work API_KEY   # load from specific profile
op-secret -r GITHUB_TOKEN   # force refresh from 1Password
```

### Automatic Shell Initialization

Cached env vars are exported from Keychain on shell startup — no 1Password API calls. SSH keys are not automatically loaded; use `op-shell` or `op-secret` to add them. Disable with:

```bash
export ZSH_OP_AUTO_EXPORT=false
```

### Using the binary directly

`zsh-op` works without the plugin, from any shell or script:

```
Usage: zsh-op <COMMAND>

Commands:
  inspect  Inspect and display the configured profiles, their secrets and their cache state.
  list     List profile names, or the secret names of a profile, one per line.
  shell    Set up the shell environment with all secrets from a profile.
  secret   Load an individual secret on demand.
  export   Export the environment and file secrets of a profile as shell statements.
  exec     Execute a command with the secrets of a profile in its environment.
  clear    Clear the cached secrets of a profile.
```

```bash
zsh-op inspect                          # show profiles, secrets and cache state
eval "$(zsh-op export -p work)"         # export a profile into bash or zsh
zsh-op export -p work --format json     # the same, as a JSON object
zsh-op exec -p work -- terraform plan   # run one command with the secrets
zsh-op clear -p work                    # delete the cached secrets of a profile
```

## How It Works

1. **Configuration** — YAML config defines profiles with `op://` secret references
2. **1Password CLI** — fetches secrets via `op read` on first load
3. **Keychain Caching** — stores secrets in macOS Keychain or the Linux Secret Service (encrypted at rest)
4. **SSH Agent** — adds SSH keys to ssh-agent with configurable expiration, piping them through `ssh-add` without writing them to disk
5. **Shell Export** — the plugin `eval`s the shell-quoted export statements printed by `zsh-op`, and exports cached env vars on shell init

Secrets are stored as `op-secrets-{profile}` / `{secret-name}`. Metadata is tracked at `~/.cache/op/{profile}.metadata`.

## Troubleshooting

**"'zsh-op' not found"** — install the binary (see [Installation](#installation)) or point `ZSH_OP_BIN` at it

**macOS asks to allow `zsh-op` access to the keychain** — secrets cached by earlier versions were stored by `/usr/bin/security`, so macOS asks once per item whether `zsh-op` may read them; choose **Always Allow**. Locally built binaries are not signed with a stable identity, so the prompt can come back after an upgrade. Alternatively, run `zsh-op clear -p <profile>` and `op-shell -r <profile>` to re-cache the secrets from 1Password.

**"Not signed in to 1Password account"** — `op signin --account my.1password.com`

**"Failed to retrieve secret"** — verify the `op://` path in your config and test with `op read "op://Vault/Item/Field"`

**"SSH agent is not running"** — `eval $(ssh-agent)`

**Secrets not auto-exporting** — ensure `op-shell` has run at least once, `ZSH_OP_AUTO_EXPORT` is not `false`, and metadata exists in `~/.cache/op/`

## The zsh-contrib Ecosystem

| Repo | What it provides |
|------|-----------------|
| [zsh-aws](https://github.com/zsh-contrib/zsh-aws) | AWS credential management with aws-vault and tmux |
| [zsh-eza](https://github.com/zsh-contrib/zsh-eza) | eza with Catppuccin and Rose Pine theming |
| [zsh-fzf](https://github.com/zsh-contrib/zsh-fzf) | fzf with Catppuccin and Rose Pine theming |
| **zsh-op** ← you are here | 1Password CLI with secure caching and SSH key management |
| [zsh-tmux](https://github.com/zsh-contrib/zsh-tmux) | Automatic tmux window title management |
| [zsh-vivid](https://github.com/zsh-contrib/zsh-vivid) | vivid LS_COLORS generation with theme support |

## License

[MIT](LICENSE) — Copyright (c) 2025 zsh-contrib

<!-- markdownlint-disable-file MD013 -->
