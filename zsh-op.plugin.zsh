#!/usr/bin/env zsh
# zsh-op.plugin.zsh - 1Password integration for zsh
#
# Wraps the secret-env binary (https://github.com/secret-env/secret-env)
# with the op-shell and op-secret commands, and exports cached secrets on
# shell initialization.

# Get plugin directory
0="${${FUNCNAME[0]:-${(%):-%x}}:A}"
ZSH_OP_PLUGIN_DIR="${0:h}"

# Default settings
: ${ZSH_OP_BIN:="secret-env"}
: ${ZSH_OP_CONFIG_FILE:="$HOME/.config/op/config.yml"}
: ${ZSH_OP_CACHE_DIR:="$HOME/.cache/op"}
: ${ZSH_OP_AUTO_EXPORT:=true}
: ${ZSH_OP_DEFAULT_PROFILE:="personal"}

# Run the secret-env binary with the plugin settings
_zsh_op() {
    if ! (( $+commands[$ZSH_OP_BIN] )) && [[ ! -x "$ZSH_OP_BIN" ]]; then
        print -u2 "zsh-op: '$ZSH_OP_BIN' not found; install secret-env (https://github.com/secret-env/secret-env)"
        return 127
    fi

    SECRET_ENV_CONFIG_FILE="$ZSH_OP_CONFIG_FILE" \
    SECRET_ENV_CACHE_DIR="$ZSH_OP_CACHE_DIR" \
    SECRET_ENV_DEFAULT_PROFILE="$ZSH_OP_DEFAULT_PROFILE" \
        command "$ZSH_OP_BIN" "$@"
}

# Get or create the private runtime directory for materialized file secrets.
# It is not exported, so child shells create (and clean up) their own.
_zsh_op_runtime_dir() {
    [[ -n "$_ZSH_OP_RUNTIME_DIR" && -d "$_ZSH_OP_RUNTIME_DIR" ]] && return 0

    local runtime_dir
    if ! runtime_dir="$(mktemp -d "${${TMPDIR:-/tmp}%/}/zsh-op.XXXXXXXXXX")"; then
        print -u2 "zsh-op: failed to create file secret runtime directory"
        return 1
    fi

    typeset -g _ZSH_OP_RUNTIME_DIR="$runtime_dir"
}

_zsh_op_cleanup_file_secrets() {
    [[ -n "$_ZSH_OP_RUNTIME_DIR" ]] || return 0

    rm -rf -- "$_ZSH_OP_RUNTIME_DIR"
    unset _ZSH_OP_RUNTIME_DIR
}

_op_shell_usage() {
    cat <<EOF
Usage: op-shell [options] [profile]

Setup your shell environment with all secrets (environment variables, file secrets, and SSH keys) from a 1Password profile.

Arguments:
  profile              Profile name (default: \$ZSH_OP_DEFAULT_PROFILE or "personal")

Options:
  -e, --expiration TIME    SSH key expiration time (default: 1h)
                           Valid formats: 30m, 1h, 8h, 24h, etc.
  -c, --config PATH        Config file path (default: ~/.config/op/config.yml)
  -r, --refresh            Force refresh from 1Password (bypass cache)
  -h, --help               Show this help message

Examples:
  op-shell                         # Setup personal profile with 1h SSH key expiration
  op-shell work                    # Setup work profile
  op-shell work -e 8h              # Setup work profile with 8h SSH key expiration
  op-shell -r personal             # Force refresh personal profile from 1Password

Environment Variables:
  ZSH_OP_DEFAULT_PROFILE          Default profile to use (default: personal)
  ZSH_OP_CONFIG_FILE              Config file location

EOF
}

# Set up the shell environment with all secrets from a profile
op-shell() {
    local -a opt_help
    zparseopts -D -E -- h=opt_help -help=opt_help

    # Handle help
    if (( $#opt_help )); then
        _op_shell_usage
        return 0
    fi

    _zsh_op_runtime_dir || return 1

    # The remaining options and the profile are passed through. Export
    # statements go to stdout and SSH keys straight to ssh-agent.
    local output rc
    output="$(_zsh_op shell --format zsh --runtime-dir "$_ZSH_OP_RUNTIME_DIR" "$@")"
    rc=$?
    eval "$output"

    return $rc
}

_op_secret_usage() {
    cat <<EOF
Usage: op-secret [options] <secret-name>

Load an individual secret (environment variable, file secret, or SSH key) on-demand.

Arguments:
  secret-name          Name of the secret to load

Options:
  -p, --profile PROFILE    Profile name (default: \$ZSH_OP_DEFAULT_PROFILE or "personal")
  -x, --export             Export environment or file secret to current shell
  -e, --expiration TIME    SSH key expiration time (default: 1h, SSH keys only)
                           Valid formats: 30m, 1h, 8h, 24h, etc.
  -r, --refresh            Force refresh from 1Password (bypass cache)
  -c, --config PATH        Config file path (default: ~/.config/op/config.yml)
  -h, --help               Show this help message

Examples:
  op-secret GITHUB_TOKEN                    # Load env secret (print value)
  op-secret GITHUB_TOKEN -x                 # Load and export env secret to shell
  op-secret GOOGLE_APPLICATION_CREDENTIALS  # Load file secret (print file path)
  op-secret github-work                     # Load SSH key with 1h expiration
  op-secret github-work -e 8h               # Load SSH key with 8h expiration
  op-secret -p work MYAPP_API_KEY           # Load secret from work profile
  op-secret -r GITHUB_TOKEN                 # Force refresh from 1Password

Environment Variables:
  ZSH_OP_DEFAULT_PROFILE          Default profile to use (default: personal)
  ZSH_OP_CONFIG_FILE              Config file location

EOF
}

# Load an individual secret on demand
op-secret() {
    local -a opt_help opt_export
    zparseopts -D -E -- x=opt_export -export=opt_export h=opt_help -help=opt_help

    # Handle help
    if (( $#opt_help )); then
        _op_secret_usage
        return 0
    fi

    _zsh_op_runtime_dir || return 1

    # Print the value (or file path) unless exporting to the current shell
    if (( ! $#opt_export )); then
        _zsh_op secret --runtime-dir "$_ZSH_OP_RUNTIME_DIR" "$@"
        return $?
    fi

    local output rc
    output="$(_zsh_op secret --export --format zsh --runtime-dir "$_ZSH_OP_RUNTIME_DIR" "$@")"
    rc=$?
    eval "$output"

    return $rc
}

# Add completions to fpath
fpath=("${ZSH_OP_PLUGIN_DIR}/completions" $fpath)
autoload -Uz _op_shell _op_secret
autoload -Uz add-zsh-hook

add-zsh-hook zshexit _zsh_op_cleanup_file_secrets

# Auto-export cached secrets on shell initialization. Only profiles loaded
# with op-shell are exported, from the keychain, without calling 1Password.
_zsh_op_auto_export() {
    [[ "$ZSH_OP_AUTO_EXPORT" == "true" ]] || return 0
    [[ -f "$ZSH_OP_CONFIG_FILE" ]] || return 0
    (( $+commands[$ZSH_OP_BIN] )) || [[ -x "$ZSH_OP_BIN" ]] || return 0
    _zsh_op_runtime_dir || return 0

    local output
    output="$(_zsh_op export --all --cached --format zsh --runtime-dir "$_ZSH_OP_RUNTIME_DIR")"
    eval "$output"
}

# Run auto-export on plugin load (suppress all output)
_zsh_op_auto_export >/dev/null 2>&1
