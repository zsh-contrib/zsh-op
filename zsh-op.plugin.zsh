#!/usr/bin/env zsh
# zsh-op.plugin.zsh - 1Password integration for zsh
#
# Main plugin entry point: wraps the zsh-op binary, sets up autoload and
# exports cached secrets on shell initialization.

# Get plugin directory
0="${${FUNCNAME[0]:-${(%):-%x}}:A}"
ZSH_OP_PLUGIN_DIR="${0:h}"

# Default settings
: ${ZSH_OP_BIN:="zsh-op"}
: ${ZSH_OP_CONFIG_FILE:="$HOME/.config/op/config.yml"}
: ${ZSH_OP_CACHE_DIR:="$HOME/.cache/op"}
: ${ZSH_OP_AUTO_EXPORT:=true}
: ${ZSH_OP_DEFAULT_PROFILE:="personal"}

# Run the zsh-op binary with the plugin settings
_zsh_op() {
    if ! (( $+commands[$ZSH_OP_BIN] )) && [[ ! -x "$ZSH_OP_BIN" ]]; then
        print -u2 "zsh-op: '$ZSH_OP_BIN' not found; install the zsh-op binary (see the README)"
        return 127
    fi

    ZSH_OP_CONFIG_FILE="$ZSH_OP_CONFIG_FILE" \
    ZSH_OP_CACHE_DIR="$ZSH_OP_CACHE_DIR" \
    ZSH_OP_DEFAULT_PROFILE="$ZSH_OP_DEFAULT_PROFILE" \
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

# Add directories to fpath for autoload and completions
fpath=("${ZSH_OP_PLUGIN_DIR}/functions" "${ZSH_OP_PLUGIN_DIR}/completions" $fpath)

# Source user commands (functions with explicit definitions)
source "${ZSH_OP_PLUGIN_DIR}/functions/op-shell"
source "${ZSH_OP_PLUGIN_DIR}/functions/op-secret"

# Autoload completion functions
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
