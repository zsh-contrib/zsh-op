# zsh-op

A Zsh plugin for seamless 1Password CLI integration. Manage environment variables and SSH keys from 1Password with automatic caching, fast shell initialization, and a configuration-driven workflow.

## Features

- Secure secret management via 1Password vaults
- Fast shell initialization with macOS Keychain caching
- Multi-profile support for separate personal and work credentials
- Multiple SSH keys per profile with independent management
- Configuration-driven setup with no hardcoded vault IDs
- Progress indicators and clear error messages via `gum`
- Dual command interface: `op-shell` for full setup, `op-secret` for individual secrets

## Requirements

- macOS (for Keychain storage)
- [1Password CLI](https://developer.1password.com/docs/cli/get-started/) (`op`)
- [gum](https://github.com/charmbracelet/gum) (for UX)
- `jq` (JSON processing)
- `python3` with PyYAML (`pip3 install PyYAML`)

## Installation

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

Create a configuration file at `~/.config/op/config.yml`:

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

  - name: work
    account: team.1password.com
    secrets:
      - kind: env
        name: MYAPP_API_KEY
        path: op://Infra/Prod/API_KEY

      - kind: ssh
        name: github-work
        path: op://Employee/GitHub SSH/private key?ssh-format=openssh

      - kind: ssh
        name: gitlab-work
        path: op://Employee/GitLab SSH/private key?ssh-format=openssh
```

See [config.example.yml](config.example.yml) for a complete example with documentation.

To find the correct `op://` path for your secrets, right-click an item in the 1Password desktop app and select **"Copy Secret Reference"**. For SSH keys, append `?ssh-format=openssh` to the path.

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `ZSH_OP_CONFIG_FILE` | `~/.config/op/config.yml` | Config file location |
| `ZSH_OP_CACHE_DIR` | `~/.cache/op` | Cache directory |
| `ZSH_OP_AUTO_EXPORT` | `true` | Auto-export env vars on shell init |
| `ZSH_OP_DEFAULT_PROFILE` | `personal` | Default profile name |
| `GUM_LOG_LEVEL` | `info` | Log level (`error`, `warn`, `info`, `debug`) |

## Usage

### `op-shell`

Setup your shell environment with all secrets from a profile.

```
Usage: op-shell [options] [profile]

Options:
  -e, --expiration TIME    SSH key expiration (default: 1h)
  -c, --config PATH        Config file path
  -r, --refresh            Force refresh from 1Password
  -h, --help               Show help
```

```bash
op-shell              # Setup default profile
op-shell work         # Setup work profile
op-shell work -e 8h   # Setup with 8-hour SSH key expiration
op-shell -r personal  # Force refresh from 1Password
```

### `op-secret`

Load an individual secret on-demand.

```
Usage: op-secret [options] <secret-name>

Options:
  -p, --profile PROFILE    Profile (default: personal)
  -x, --export             Export env secret to shell
  -e, --expiration TIME    SSH key expiration (default: 1h)
  -r, --refresh            Force refresh from 1Password
  -c, --config PATH        Config file path
  -h, --help               Show help
```

```bash
op-secret GITHUB_TOKEN      # Load and print a secret
op-secret GITHUB_TOKEN -x   # Export to current shell
op-secret github-work       # Load an SSH key
op-secret -p work API_KEY   # Load from specific profile
op-secret -r GITHUB_TOKEN   # Force refresh from 1Password
```

### Automatic Shell Initialization

Cached environment variables are automatically exported from Keychain on shell startup (no 1Password API calls). SSH keys are not automatically loaded — use `op-shell` or `op-secret` to add them to your ssh-agent.

To disable auto-export:

```bash
export ZSH_OP_AUTO_EXPORT=false
```

## How It Works

1. **Configuration** — YAML config defines profiles with secret references
2. **1Password CLI** — Fetches secrets via `op read` on first load
3. **Keychain Caching** — Stores secrets in macOS Keychain (encrypted at rest)
4. **SSH Agent** — Adds SSH keys to ssh-agent with configurable expiration
5. **Shell Export** — Automatically exports cached env vars on shell init

Secrets are stored in Keychain as `op-secrets-{profile}` / `{secret-name}`. Metadata is tracked at `~/.cache/op/{profile}.metadata`.

## Troubleshooting

**"python3 is required but not found"** — Install with `brew install python3`.

**"PyYAML module is required"** — Install with `pip3 install PyYAML`.

**"Not signed in to 1Password account"** — Run `op signin --account my.1password.com`.

**"Failed to retrieve secret"** — Verify the `op://` path in your config, ensure vault access, and test with `op read "op://Vault/Item/Field"`.

**"SSH agent is not running"** — Start with `eval $(ssh-agent)`.

**Secrets not auto-exporting** — Ensure `op-shell` has been run at least once, `ZSH_OP_AUTO_EXPORT` is not `false`, and metadata exists in `~/.cache/op/`.

**Debug logging** — Set `GUM_LOG_LEVEL=debug` or use `DEBUG=1` before any command for verbose output.

## Directory Structure

```
zsh-op/
  completions/       # Zsh completion definitions
  functions/         # Plugin function files
  lib/               # Shared library scripts
  config.example.yml # Example configuration file
  zsh-op.plugin.zsh  # Plugin entry point
```

## License

MIT License - see [LICENSE](./LICENSE) for details.
