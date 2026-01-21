#!/usr/bin/env zsh
# lib/keychain.zsh - macOS Keychain operations

# Write a secret to macOS Keychain
# Usage: _zsh_op_keychain_write <service> <account> <value>
_zsh_op_keychain_write() {
    local service="$1"
    local account="$2"
    local value="$3"

    if [[ -z "$service" || -z "$account" || -z "$value" ]]; then
        gum log --level error "keychain_write: missing required arguments"
        return 1
    fi

    # Use -U flag to update if exists, create if not
    if ! security add-generic-password -U -s "$service" -a "$account" -w "$value" 2>/dev/null; then
        gum log --level error "Failed to write to Keychain"
        gum log --level warn "Service: $service, Account: $account"
        return 1
    fi

    return 0
}

# Read a secret from macOS Keychain
# Usage: _zsh_op_keychain_read <service> <account>
_zsh_op_keychain_read() {
    local service="$1"
    local account="$2"

    if [[ -z "$service" || -z "$account" ]]; then
        gum log --level error "keychain_read: missing required arguments"
        return 1
    fi

    # Read password from keychain (-w flag outputs only the password)
    local value
    if ! value=$(security find-generic-password -s "$service" -a "$account" -w 2>/dev/null); then
        return 1
    fi

    echo "$value"
    return 0
}

# Delete a secret from macOS Keychain
# Usage: _zsh_op_keychain_delete <service> <account>
_zsh_op_keychain_delete() {
    local service="$1"
    local account="$2"

    if [[ -z "$service" || -z "$account" ]]; then
        gum log --level error "keychain_delete: missing required arguments"
        return 1
    fi

    # Delete password from keychain
    if ! security delete-generic-password -s "$service" -a "$account" 2>/dev/null; then
        # Not an error if the item doesn't exist
        return 0
    fi

    return 0
}

# Check if a secret exists in macOS Keychain
# Usage: _zsh_op_keychain_exists <service> <account>
_zsh_op_keychain_exists() {
    local service="$1"
    local account="$2"

    if [[ -z "$service" || -z "$account" ]]; then
        return 1
    fi

    # Check if password exists in keychain
    security find-generic-password -s "$service" -a "$account" >/dev/null 2>&1
    return $?
}

# List all accounts for a service in macOS Keychain
# Usage: _zsh_op_keychain_list <service>
_zsh_op_keychain_list() {
    local service="$1"

    if [[ -z "$service" ]]; then
        gum log --level error "keychain_list: missing service argument"
        return 1
    fi

    # Use security dump-keychain and parse output
    # This is a bit of a hack, but it works for listing accounts
    security dump-keychain 2>/dev/null | \
        grep -A 5 "svce.*${service}" | \
        grep "acct" | \
        sed 's/.*"\(.*\)".*/\1/'
}

# Clear all cached secrets for a profile
# Usage: _zsh_op_keychain_clear_profile <profile>
_zsh_op_keychain_clear_profile() {
    local profile="$1"

    if [[ -z "$profile" ]]; then
        gum log --level error "keychain_clear_profile: missing profile argument"
        return 1
    fi

    local service="op-secrets-${profile}"
    local metadata_file="${ZSH_OP_CACHE_DIR}/${profile}.metadata"

    # Read metadata to get list of cached secrets
    if [[ ! -f "$metadata_file" ]]; then
        gum log --level warn "No cached secrets found for profile: $profile"
        return 0
    fi

    local count=0
    local line secret_name

    while IFS= read -r line; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "$line" ]] && continue

        # Parse: type:name (e.g., "env:GITHUB_TOKEN" or "ssh:github-work")
        secret_name="${line#*:}"

        # Delete from keychain
        if _zsh_op_keychain_delete "$service" "$secret_name"; then
            ((count++))
        fi
    done < "$metadata_file"

    # Remove metadata file
    rm -f "$metadata_file"

    gum log --level info "Cleared $count cached secret(s) for profile: $profile"
    return 0
}
