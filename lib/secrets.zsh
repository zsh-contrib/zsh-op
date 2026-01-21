#!/usr/bin/env zsh
# lib/secrets.zsh - Environment secret management

# Load an environment secret from 1Password and cache in Keychain
# Usage: _zsh_op_load_env_secret <profile> <secret_name> [refresh]
_zsh_op_load_env_secret() {
    local profile="$1"
    local secret_name="$2"
    local refresh="${3:-false}"

    if [[ -z "$profile" || -z "$secret_name" ]]; then
        gum log --level error "load_env_secret: missing required arguments"
        return 1
    fi

    # Get secret config
    local key="${profile}:${secret_name}"
    local op_path="${ZSH_OP_SECRETS[$key]}"
    local kind="${ZSH_OP_SECRET_KINDS[$key]}"

    if [[ -z "$op_path" ]]; then
        gum log --level error "Secret '$secret_name' not found in profile '$profile'"
        return 1
    fi

    if [[ "$kind" != "env" ]]; then
        gum log --level error "Secret '$secret_name' is not an environment secret (kind: $kind)"
        return 1
    fi

    local service="op-secrets-${profile}"
    local value

    # Check cache unless refresh is requested
    if [[ "$refresh" == "false" ]]; then
        if value=$(_zsh_op_keychain_read "$service" "$secret_name" 2>/dev/null); then
            gum log --level debug "Loaded '$secret_name' from cache"
            echo "$value"
            return 0
        fi
    fi

    # Fetch from 1Password
    local account_url="${ZSH_OP_ACCOUNTS[$profile]}"

    # Check if signed in to 1Password
    if ! op account get --account "$account_url" >/dev/null 2>&1; then
        gum log --level error "Not signed in to 1Password account: $account_url"
        gum log --level info "Run: op signin --account $account_url"
        return 1
    fi

    # Retrieve secret with spinner
    if ! value=$(gum spin --title "Retrieving '$secret_name' from 1Password..." --show-stderr -- \
        op read "$op_path" --account "$account_url"); then
        gum log --level error "Failed to retrieve secret '$secret_name'"
        gum log --level warn "Path: $op_path"
        return 1
    fi

    if [[ -z "$value" ]]; then
        gum log --level error "Secret '$secret_name' is empty"
        return 1
    fi

    # Cache in keychain
    if ! _zsh_op_keychain_write "$service" "$secret_name" "$value"; then
        gum log --level warn "Failed to cache secret in Keychain"
        # Continue anyway - we have the value
    fi

    echo "$value"
    return 0
}

# Export an environment secret to the current shell
# Usage: _zsh_op_export_env_secret <profile> <secret_name> [refresh]
_zsh_op_export_env_secret() {
    local profile="$1"
    local secret_name="$2"
    local refresh="${3:-false}"

    local value
    if ! value=$(_zsh_op_load_env_secret "$profile" "$secret_name" "$refresh"); then
        return 1
    fi

    # Export to current shell
    export "${secret_name}=${value}"

    gum log --level debug "Exported '$secret_name'"
    return 0
}

# Load all environment secrets for a profile from cache only
# Usage: _zsh_op_export_cached_secrets <profile>
_zsh_op_export_cached_secrets() {
    local profile="$1"

    if [[ -z "$profile" ]]; then
        gum log --level error "export_cached_secrets: missing profile argument"
        return 1
    fi

    local service="op-secrets-${profile}"
    local metadata_file="${ZSH_OP_CACHE_DIR}/${profile}.metadata"

    # Skip if no metadata (profile never loaded)
    if [[ ! -f "$metadata_file" ]]; then
        gum log --level debug "No cached secrets for profile: $profile"
        return 0
    fi

    local count=0
    local line secret_name value

    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "$line" ]] && continue

        # Parse: type:name (e.g., "env:GITHUB_TOKEN")
        local secret_type="${line%%:*}"
        secret_name="${line#*:}"

        # Only export env secrets
        [[ "$secret_type" == "env" ]] || continue

        # Read from keychain and export
        if value=$(_zsh_op_keychain_read "$service" "$secret_name" 2>/dev/null); then
            export "${secret_name}=${value}"
            ((count++))
            gum log --level debug "Exported '$secret_name' from cache"
        fi
    done < "$metadata_file"

    gum log --level debug "Exported $count cached secret(s) for profile: $profile"
    return 0
}

# Load all environment secrets for a profile
# Usage: _zsh_op_load_all_env_secrets <profile> [refresh]
_zsh_op_load_all_env_secrets() {
    local profile="$1"
    local refresh="${2:-false}"

    if [[ -z "$profile" ]]; then
        gum log --level error "load_all_env_secrets: missing profile argument"
        return 1
    fi

    # Get all env secrets for this profile
    local key secret_name kind
    local loaded=0
    local failed=0

    for key in ${(k)ZSH_OP_SECRETS}; do
        # Skip if not for this profile
        [[ "$key" =~ ^${profile}: ]] || continue

        kind="${ZSH_OP_SECRET_KINDS[$key]}"
        secret_name="${ZSH_OP_SECRET_NAMES[$key]}"

        # Only load env secrets
        [[ "$kind" == "env" ]] || continue

        # Load and export
        if _zsh_op_export_env_secret "$profile" "$secret_name" "$refresh"; then
            ((loaded++))
        else
            ((failed++))
        fi
    done

    if [[ $failed -gt 0 ]]; then
        gum log --level warn "Failed to load $failed environment secret(s)"
    fi

    if [[ $loaded -gt 0 ]]; then
        gum log --level info "Loaded $loaded environment secret(s)"
    fi

    return 0
}

# Save metadata about loaded secrets for auto-export
# Usage: _zsh_op_save_metadata <profile>
_zsh_op_save_metadata() {
    local profile="$1"

    if [[ -z "$profile" ]]; then
        gum log --level error "save_metadata: missing profile argument"
        return 1
    fi

    # Ensure cache directory exists
    mkdir -p "$ZSH_OP_CACHE_DIR"

    local metadata_file="${ZSH_OP_CACHE_DIR}/${profile}.metadata"
    local key kind name

    # Write metadata file
    {
        echo "# zsh-op metadata for profile: $profile"
        echo "# Format: type:name"
        echo "# Generated: $(date)"
        echo ""

        for key in ${(k)ZSH_OP_SECRETS}; do
            # Skip if not for this profile
            [[ "$key" =~ ^${profile}: ]] || continue

            kind="${ZSH_OP_SECRET_KINDS[$key]}"
            name="${ZSH_OP_SECRET_NAMES[$key]}"

            echo "${kind}:${name}"
        done
    } > "$metadata_file"

    return 0
}
