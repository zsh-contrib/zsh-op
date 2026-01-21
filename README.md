# zsh-op

A zsh plugin for seamless 1Password CLI integration. Manage your environment variables and SSH keys from 1Password with automatic caching, fast shell initialization, and a clean configuration-driven workflow.

## Features

- 🔐 **Secure secret management** - Store API keys, tokens, and SSH keys in 1Password
- ⚡ **Fast shell initialization** - Cached secrets load instantly from macOS Keychain
- 🎯 **Multi-profile support** - Separate personal and work credentials
- 🔑 **Multiple SSH keys per profile** - Name and manage multiple SSH keys independently
- 📝 **Configuration-driven** - No hardcoded vault IDs or item paths
- 🎨 **Beautiful UX** - Progress indicators and clear error messages via `gum`
- 🔄 **Dual command interface** - Bulk loading with `op-auth`, on-demand with `op-load`

## Requirements

- macOS (for Keychain storage)
- [1Password CLI](https://developer.1password.com/docs/cli/get-started/) (`op`)
- [gum](https://github.com/charmbracelet/gum) (for UX)
- `jq` (JSON processing)
- `python3` with PyYAML (`pip3 install PyYAML`)

### Installation

**Install dependencies:**

```bash
# Install 1Password CLI
brew install 1password-cli

# Install gum
brew install gum

# Install jq
brew install jq

# Install PyYAML
pip3 install PyYAML
```

## Plugin Installation

Add the following to your `~/.zshrc`:

```bash
zinit light zsh-contrib/zsh-op
```

Then reload your shell:

```bash
source ~/.zshrc
```

## Configuration

### 1. Create Configuration Directory

```bash
mkdir -p ~/.config/op
```

### 2. Create Configuration File

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

**See [config.example.yml](config.example.yml) for a complete example with documentation.**

### 3. Get Secret References from 1Password

To find the correct `op://` path for your secrets:

1. Open 1Password desktop app
2. Right-click the item containing your secret
3. Select **"Copy Secret Reference"**
4. Paste into your `config.yml` as the `path` value

For SSH keys, append `?ssh-format=openssh` to the path.

## Usage

### Load All Secrets for a Profile

```bash
# Load personal profile (default)
op-auth

# Load work profile
op-auth work

# Load with 8-hour SSH key expiration
op-auth work -e 8h

# Force refresh from 1Password (bypass cache)
op-auth -r personal
```

### Load Individual Secrets

```bash
# Load and print an environment variable
op-load GITHUB_TOKEN

# Load and export to current shell
op-load GITHUB_TOKEN -x

# Load an SSH key
op-load github-work

# Load from specific profile
op-load -p work MYAPP_API_KEY

# Force refresh from 1Password
op-load -r GITHUB_TOKEN
```

### Automatic Shell Initialization

When you start a new shell, cached environment variables are automatically exported from Keychain (no 1Password API calls). SSH keys are NOT automatically loaded - use `op-auth` or `op-load` to add them to your ssh-agent.

To disable auto-export:

```bash
export ZSH_OP_AUTO_EXPORT=false
```

## Commands

### `op-auth`

Load all secrets (environment variables and SSH keys) for a profile.

```
Usage: op-auth [options] [profile]

Options:
  -e, --expiration TIME    SSH key expiration (default: 1h)
  -c, --config PATH        Config file path
  -r, --refresh            Force refresh from 1Password
  -h, --help               Show help
```

### `op-load`

Load an individual secret on-demand.

```
Usage: op-load [options] <secret-name>

Options:
  -p, --profile PROFILE    Profile (default: personal)
  -x, --export             Export env secret to shell
  -e, --expiration TIME    SSH key expiration (default: 1h)
  -r, --refresh            Force refresh from 1Password
  -c, --config PATH        Config file path
  -h, --help               Show help
```

## Environment Variables

- `ZSH_OP_CONFIG_FILE` - Config file location (default: `~/.config/op/config.yml`)
- `ZSH_OP_CACHE_DIR` - Cache directory (default: `~/.cache/op`)
- `ZSH_OP_AUTO_EXPORT` - Auto-export on shell init (default: `true`)
- `ZSH_OP_DEFAULT_PROFILE` - Default profile name (default: `personal`)
- `DEBUG` - Enable debug output (default: unset)

## How It Works

### Architecture

1. **Configuration** - YAML config defines profiles and secrets
2. **1Password Integration** - Fetches secrets via `op` CLI
3. **Keychain Caching** - Stores secrets in macOS Keychain
4. **SSH Agent** - Adds SSH keys with expiration
5. **Shell Export** - Automatically exports env vars on shell init

### Storage Pattern

Secrets are stored in macOS Keychain with this pattern:

```
Service: op-secrets-{profile}  (e.g., "op-secrets-work")
Account: {secret-name}          (e.g., "GITHUB_TOKEN" or "github-work")
Password: {secret-value}        (actual credential)
```

Metadata files track loaded secrets at: `~/.cache/op/{profile}.metadata`

### Performance

- **Shell initialization**: < 50ms (Keychain reads only)
- **First load**: 1-2s per secret (1Password API calls)
- **Cached load**: < 100ms (Keychain reads)

## Troubleshooting

### "python3 is required but not found"

Install Python 3:

```bash
brew install python3
```

### "PyYAML module is required"

Install PyYAML:

```bash
pip3 install PyYAML
```

### "Not signed in to 1Password account"

Sign in to 1Password:

```bash
op signin --account my.1password.com
```

### "Failed to retrieve secret"

Check your secret path:

1. Verify the `op://` path in your config
2. Ensure you have access to the vault
3. Try manually: `op read "op://Vault/Item/Field"`

### "SSH agent is not running"

Start the SSH agent:

```bash
eval $(ssh-agent)
```

Or add to your `~/.zshrc`:

```bash
# Start ssh-agent if not running
if ! pgrep -u "$USER" ssh-agent > /dev/null; then
    eval "$(ssh-agent -s)"
fi
```

### Secrets not auto-exporting

1. Check that you've run `op-auth` at least once for your profile
2. Verify `ZSH_OP_AUTO_EXPORT` is not set to `false`
3. Check metadata file exists: `ls ~/.cache/op/`
4. Enable debug mode: `DEBUG=1 zsh`

### Clear cached secrets

To clear cached secrets for a profile:

```bash
# This will remove from Keychain and delete metadata
rm ~/.cache/op/personal.metadata
security delete-generic-password -s "op-secrets-personal" -a "SECRET_NAME"
```

## Security Considerations

- **Keychain encryption**: Secrets are encrypted at rest by macOS Keychain
- **SSH key expiration**: Keys automatically expire from ssh-agent
- **Temporary files**: SSH keys use 600 permissions and are cleaned up immediately
- **No environment pollution**: Secrets aren't exported until explicitly loaded
- **Cached credentials**: Stored securely in Keychain, not in plain text

## Advanced Configuration

### Custom Config Location

```bash
export ZSH_OP_CONFIG_FILE="$HOME/my-configs/op-config.yml"
```

### Multiple SSH Keys Per Profile

```yaml
accounts:
  - name: work
    account: team.1password.com
    secrets:
      - kind: ssh
        name: github-work
        path: op://Employee/GitHub SSH/private key?ssh-format=openssh

      - kind: ssh
        name: gitlab-work
        path: op://Employee/GitLab SSH/private key?ssh-format=openssh

      - kind: ssh
        name: bastion
        path: op://Infra/Bastion/private key?ssh-format=openssh
```

Load individual keys:

```bash
op-load github-work
op-load gitlab-work -e 4h
```

### Profile-Specific Defaults

Set different defaults per shell:

```bash
# In work shell
export ZSH_OP_DEFAULT_PROFILE="work"

# Now these use work profile by default
op-load MYAPP_API_KEY
```

## Contributing

Contributions are welcome! Please open an issue or pull request.

## License

MIT

## Credits

- Built with [1Password CLI](https://developer.1password.com/docs/cli/)
- UX powered by [gum](https://github.com/charmbracelet/gum)
- Inspired by the need for better secret management in development workflows
