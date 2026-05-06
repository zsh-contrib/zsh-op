#!/usr/bin/env zsh
# lib/secrets.zsh - Environment secret management

# Load an environment secret from 1Password and cache in Keychain
# Usage: _zsh_op_secret_env <profile> <secret_name> [refresh]
_zsh_op_secret_env() {
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
    if ! value=$(_zsh_op_secret_env "$profile" "$secret_name" "$refresh"); then
        return 1
    fi

    # Export to current shell
    export "${secret_name}=${value}"

    gum log --level debug "Exported '$secret_name'"
    return 0
}

# Get or create the secure runtime directory for materialized file secrets
_zsh_op_runtime_dir() {
    if [[ -z "$_ZSH_OP_RUNTIME_DIR" ]]; then
        local tmp_root="${TMPDIR:-/tmp}"
        tmp_root="${tmp_root%/}"

        local runtime_dir
        if ! runtime_dir="$(mktemp -d "${tmp_root}/zsh-op.XXXXXXXXXX")"; then
            gum log --level error "Failed to create file secret runtime directory"
            return 1
        fi

        typeset -g _ZSH_OP_RUNTIME_DIR="$runtime_dir"
    fi

    if [[ -z "$_ZSH_OP_RUNTIME_DIR" ]]; then
        gum log --level error "Failed to create file secret runtime directory"
        return 1
    fi

    if ! mkdir -p "$_ZSH_OP_RUNTIME_DIR"; then
        gum log --level error "Failed to create file secret runtime directory: $_ZSH_OP_RUNTIME_DIR"
        return 1
    fi

    chmod 700 "$_ZSH_OP_RUNTIME_DIR" 2>/dev/null || true
    echo "$_ZSH_OP_RUNTIME_DIR"
    return 0
}

# Write secret content to a secure runtime file and return its path
# Usage: _zsh_op_write_secret_file <profile> <secret_name> <value>
_zsh_op_write_secret_file() {
    local profile="$1"
    local secret_name="$2"
    local value="$3"

    if [[ -z "$profile" || -z "$secret_name" ]]; then
        gum log --level error "write_secret_file: missing required arguments"
        return 1
    fi

    if ! _zsh_op_runtime_dir >/dev/null; then
        return 1
    fi

    local runtime_dir="$_ZSH_OP_RUNTIME_DIR"
    local file_dir="${runtime_dir}/files/${profile}"
    local file_path="${file_dir}/${secret_name}"

    if ! mkdir -p "$file_dir"; then
        gum log --level error "Failed to create file secret runtime directory: $file_dir"
        return 1
    fi

    chmod 700 "$file_dir" 2>/dev/null || true

    if ! printf "%s" "$value" > "$file_path"; then
        gum log --level error "Failed to write file secret: $file_path"
        return 1
    fi

    chmod 600 "$file_path" 2>/dev/null || true
    echo "$file_path"
    return 0
}

# Load a file secret from 1Password, write it to a file, and cache in Keychain
# Usage: _zsh_op_secret_file <profile> <secret_name> [refresh]
_zsh_op_secret_file() {
    local profile="$1"
    local secret_name="$2"
    local refresh="${3:-false}"

    if [[ -z "$profile" || -z "$secret_name" ]]; then
        gum log --level error "load_file_secret: missing required arguments"
        return 1
    fi

    local key="${profile}:${secret_name}"
    local op_path="${ZSH_OP_SECRETS[$key]}"
    local kind="${ZSH_OP_SECRET_KINDS[$key]}"

    if [[ -z "$op_path" ]]; then
        gum log --level error "Secret '$secret_name' not found in profile '$profile'"
        return 1
    fi

    if [[ "$kind" != "file" ]]; then
        gum log --level error "Secret '$secret_name' is not a file secret (kind: $kind)"
        return 1
    fi

    local service="op-secrets-${profile}"
    local value file_path

    # Check cache unless refresh is requested
    if [[ "$refresh" == "false" ]]; then
        if value=$(_zsh_op_keychain_read "$service" "$secret_name" 2>/dev/null); then
            gum log --level debug "Loaded '$secret_name' from cache"
            _zsh_op_write_secret_file "$profile" "$secret_name" "$value"
            return $?
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
    if ! value=$(gum spin --title "Retrieving file secret '$secret_name' from 1Password..." --show-stderr -- \
        op read "$op_path" --account "$account_url"); then
        gum log --level error "Failed to retrieve file secret '$secret_name'"
        gum log --level warn "Path: $op_path"
        return 1
    fi

    if [[ -z "$value" ]]; then
        gum log --level error "File secret '$secret_name' is empty"
        return 1
    fi

    # Cache in keychain
    if ! _zsh_op_keychain_write "$service" "$secret_name" "$value"; then
        gum log --level warn "Failed to cache file secret in Keychain"
        # Continue anyway - we have the value
    fi

    _zsh_op_write_secret_file "$profile" "$secret_name" "$value"
    return $?
}

# Export a file secret path to the current shell
# Usage: _zsh_op_export_file_secret <profile> <secret_name> [refresh]
_zsh_op_export_file_secret() {
    local profile="$1"
    local secret_name="$2"
    local refresh="${3:-false}"

    local file_path
    if ! file_path=$(_zsh_op_secret_file "$profile" "$secret_name" "$refresh"); then
        return 1
    fi

    export "${secret_name}=${file_path}"

    gum log --level debug "Exported '$secret_name' file path"
    return 0
}

# Resolve cached metadata through the currently loaded config when possible.
# This keeps auto-export correct if a secret changes kind between env and file.
_zsh_op_cached_secret_type() {
    local profile="$1"
    local secret_name="$2"
    local metadata_type="$3"
    local key="${profile}:${secret_name}"
    local config_type=""

    if [[ "${(t)ZSH_OP_SECRET_KINDS}" == *association* ]]; then
        config_type="${ZSH_OP_SECRET_KINDS[$key]}"
    fi

    if [[ "$config_type" == "env" || "$config_type" == "file" ]]; then
        echo "$config_type"
    else
        echo "$metadata_type"
    fi
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
        local metadata_type="${line%%:*}"
        secret_name="${line#*:}"
        local secret_type="$(_zsh_op_cached_secret_type "$profile" "$secret_name" "$metadata_type")"

        case "$secret_type" in
        env)
            if value=$(_zsh_op_keychain_read "$service" "$secret_name" 2>/dev/null); then
                export "${secret_name}=${value}"
                ((count++))
                gum log --level debug "Exported '$secret_name' from cache"
            fi
            ;;
        file)
            if value=$(_zsh_op_keychain_read "$service" "$secret_name" 2>/dev/null); then
                local file_path
                if file_path=$(_zsh_op_write_secret_file "$profile" "$secret_name" "$value"); then
                    export "${secret_name}=${file_path}"
                    ((count++))
                    gum log --level debug "Exported '$secret_name' file path from cache"
                fi
            fi
            ;;
        esac
    done < "$metadata_file"

    gum log --level debug "Exported $count cached secret(s) for profile: $profile"
    return 0
}

# Load all environment secrets for a profile
# Usage: _zsh_op_secret_all_env <profile> [refresh]
_zsh_op_secret_all_env() {
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

# Load all file secrets for a profile
# Usage: _zsh_op_secret_all_files <profile> [refresh]
_zsh_op_secret_all_files() {
    local profile="$1"
    local refresh="${2:-false}"

    if [[ -z "$profile" ]]; then
        gum log --level error "load_all_file_secrets: missing profile argument"
        return 1
    fi

    local key secret_name kind
    local loaded=0
    local failed=0

    for key in ${(k)ZSH_OP_SECRETS}; do
        # Skip if not for this profile
        [[ "$key" =~ ^${profile}: ]] || continue

        kind="${ZSH_OP_SECRET_KINDS[$key]}"
        secret_name="${ZSH_OP_SECRET_NAMES[$key]}"

        # Only load file secrets
        [[ "$kind" == "file" ]] || continue

        # Load, write to file, and export path
        if _zsh_op_export_file_secret "$profile" "$secret_name" "$refresh"; then
            ((loaded++))
        else
            ((failed++))
        fi
    done

    if [[ $failed -gt 0 ]]; then
        gum log --level warn "Failed to load $failed file secret(s)"
    fi

    if [[ $loaded -gt 0 ]]; then
        gum log --level info "Loaded $loaded file secret(s)"
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
