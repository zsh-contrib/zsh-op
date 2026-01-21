#!/usr/bin/env zsh
# zsh-op.plugin.zsh - 1Password integration for zsh
#
# Main plugin entry point that loads libraries, sets up autoload,
# and handles auto-export of cached secrets on shell initialization.

# Get plugin directory
0="${${FUNCNAME[0]:-${(%):-%x}}:A}"
ZSH_OP_PLUGIN_DIR="${0:h}"

# Global configuration variables
typeset -gA ZSH_OP_ACCOUNTS      # profile -> account-url
typeset -gA ZSH_OP_SECRETS       # profile:name -> op-path
typeset -gA ZSH_OP_SECRET_KINDS  # profile:name -> (env|ssh)
typeset -gA ZSH_OP_SECRET_NAMES  # profile:name -> name

# Default settings
: ${ZSH_OP_CONFIG_FILE:="$HOME/.config/op/config.yml"}
: ${ZSH_OP_CACHE_DIR:="$HOME/.cache/op"}
: ${ZSH_OP_AUTO_EXPORT:=true}
: ${ZSH_OP_DEFAULT_PROFILE:="personal"}
: ${GUM_LOG_LEVEL:="info"}

# Export GUM_LOG_LEVEL so gum can see it
export GUM_LOG_LEVEL

# Load library files
source "${ZSH_OP_PLUGIN_DIR}/lib/config.zsh"
source "${ZSH_OP_PLUGIN_DIR}/lib/keychain.zsh"
source "${ZSH_OP_PLUGIN_DIR}/lib/secrets.zsh"
source "${ZSH_OP_PLUGIN_DIR}/lib/ssh.zsh"

# Add directories to fpath for autoload
fpath=("${ZSH_OP_PLUGIN_DIR}/functions" "${ZSH_OP_PLUGIN_DIR}/completions" $fpath)

# Autoload user commands
autoload -Uz op-auth op-load

# Autoload completion functions
autoload -Uz _op_auth _op_load

# Auto-export cached secrets on shell initialization
_zsh_op_auto_export() {
    # Skip if disabled
    [[ "$ZSH_OP_AUTO_EXPORT" == "true" ]] || return 0

    # Skip if config doesn't exist
    [[ -f "$ZSH_OP_CONFIG_FILE" ]] || return 0

    # Load config to get profiles
    _zsh_op_load_config "$ZSH_OP_CONFIG_FILE" 2>/dev/null || return 0

    # Export cached secrets for each profile
    local profile
    for profile in ${(k)ZSH_OP_ACCOUNTS}; do
        local metadata_file="${ZSH_OP_CACHE_DIR}/${profile}.metadata"

        # Skip if no metadata (profile never loaded)
        [[ -f "$metadata_file" ]] || continue

        # Read metadata to get list of cached env secrets
        local service="op-secrets-${profile}"
        local line secret_name

        while IFS= read -r line; do
            # Skip comments and empty lines
            [[ "$line" =~ ^[[:space:]]*# ]] && continue
            [[ -z "$line" ]] && continue

            # Parse: type:name (e.g., "env:GITHUB_TOKEN" or "ssh:github-work")
            local secret_type="${line%%:*}"
            secret_name="${line#*:}"

            # Only export env secrets
            [[ "$secret_type" == "env" ]] || continue

            # Read from keychain and export
            local value
            if value=$(_zsh_op_keychain_read "$service" "$secret_name" 2>/dev/null); then
                export "${secret_name}=${value}"
                gum log --level debug "Exported '${secret_name}' from cache"
            fi
        done < "$metadata_file"
    done
}

# Run auto-export on plugin load (suppress all output)
_zsh_op_auto_export >/dev/null 2>&1
