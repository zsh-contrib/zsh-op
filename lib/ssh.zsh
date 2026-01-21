#!/usr/bin/env zsh
# lib/ssh.zsh - SSH key management with 1Password integration

# Load an SSH key from 1Password and add to ssh-agent
# Usage: _zsh_op_secret_ssh <profile> <key_name> <expiration> [refresh]
_zsh_op_secret_ssh() {
    local profile="$1"
    local key_name="$2"
    local expiration="${3:-1h}"
    local refresh="${4:-false}"

    if [[ -z "$profile" || -z "$key_name" ]]; then
        gum log --level error "load_ssh_key: missing required arguments"
        return 1
    fi

    # Get secret config
    local key="${profile}:${key_name}"
    local op_path="${ZSH_OP_SECRETS[$key]}"
    local kind="${ZSH_OP_SECRET_KINDS[$key]}"

    if [[ -z "$op_path" ]]; then
        gum log --level error "SSH key '$key_name' not found in profile '$profile'"
        return 1
    fi

    if [[ "$kind" != "ssh" ]]; then
        gum log --level error "Secret '$key_name' is not an SSH key (kind: $kind)"
        return 1
    fi

    local service="op-secrets-${profile}"
    local key_data

    # Check cache unless refresh is requested
    if [[ "$refresh" == "false" ]]; then
        if key_data=$(_zsh_op_keychain_read "$service" "$key_name" 2>/dev/null); then
            gum log --level debug "Loaded '$key_name' from cache"
            # Add to ssh-agent from cache
            _zsh_op_add_ssh_key_to_agent "$key_name" "$key_data" "$expiration" "$refresh"
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

    # Retrieve SSH key with spinner
    if ! key_data=$(gum spin --title "Retrieving SSH key '$key_name' from 1Password..." --show-stderr -- \
        op read "$op_path" --account "$account_url"); then
        gum log --level error "Failed to retrieve SSH key '$key_name'"
        gum log --level warn "Path: $op_path"
        return 1
    fi

    if [[ -z "$key_data" ]]; then
        gum log --level error "SSH key '$key_name' is empty"
        return 1
    fi

    # Cache in keychain
    if ! _zsh_op_keychain_write "$service" "$key_name" "$key_data"; then
        gum log --level warn "Failed to cache SSH key in Keychain"
        # Continue anyway - we have the key
    fi

    # Add to ssh-agent
    _zsh_op_add_ssh_key_to_agent "$key_name" "$key_data" "$expiration" "$refresh"
    return $?
}

# Add SSH key to ssh-agent with expiration
# Usage: _zsh_op_add_ssh_key_to_agent <key_name> <key_data> <expiration> [refresh]
_zsh_op_add_ssh_key_to_agent() {
    local key_name="$1"
    local key_data="$2"
    local expiration="$3"
    local refresh="${4:-false}"

    # Create temporary file with 600 permissions
    local key_path
    key_path=$(mktemp -t "ssh-${key_name}")

    # Ensure cleanup on exit
    trap "rm -f '$key_path'" EXIT INT TERM

    # Write key to temp file
    echo "$key_data" > "$key_path"
    chmod 600 "$key_path"

    # Check if ssh-agent is running
    # ssh-add -l exit codes:
    #   0 = agent running with keys
    #   1 = agent running without keys
    #   2 = agent not running
    ssh-add -l >/dev/null 2>&1
    local agent_status=$?
    if [[ $agent_status -eq 2 ]]; then
        gum log --level error "SSH agent is not running"
        gum log --level info "Start with: eval \$(ssh-agent)"
        rm -f "$key_path"
        return 1
    fi

    # Get fingerprint of the key we want to add
    local key_fingerprint
    key_fingerprint=$(ssh-keygen -lf "$key_path" 2>/dev/null | awk '{print $2}')

    gum log --level debug "Key fingerprint: $key_fingerprint"

    # Check if key is already in the agent
    if [[ -n "$key_fingerprint" ]] && ssh-add -l 2>/dev/null | grep -q "$key_fingerprint"; then
        if [[ "$refresh" == "true" ]]; then
            gum log --level debug "Removing existing key '$key_name' from agent (refresh requested)"
            # Delete by public key
            ssh-add -d "$key_path" >/dev/null 2>&1 || true
        else
            gum log --level debug "SSH key '$key_name' already in agent (use --refresh to reset expiration)"
            rm -f "$key_path"
            return 0
        fi
    else
        gum log --level debug "Key not found in agent, adding..."
    fi

    # Add key to ssh-agent with expiration
    local ssh_add_output
    if ! ssh_add_output=$(ssh-add -t "${expiration}" "$key_path" 2>&1); then
        gum log --level error "Failed to add SSH key to agent"
        gum log --level debug "ssh-add output: $ssh_add_output"
        rm -f "$key_path"
        return 1
    fi

    # Cleanup temp file
    rm -f "$key_path"

    gum log --level debug "Added '$key_name' to ssh-agent"
    return 0
}

# Load all SSH keys for a profile
# Usage: _zsh_op_load_all_ssh_keys <profile> <expiration> [refresh]
_zsh_op_load_all_ssh_keys() {
    local profile="$1"
    local expiration="${2:-1h}"
    local refresh="${3:-false}"

    if [[ -z "$profile" ]]; then
        gum log --level error "load_all_ssh_keys: missing profile argument"
        return 1
    fi

    # Get all SSH keys for this profile
    local key secret_name kind
    local loaded=0
    local failed=0

    for key in ${(k)ZSH_OP_SECRETS}; do
        # Skip if not for this profile
        [[ "$key" =~ ^${profile}: ]] || continue

        kind="${ZSH_OP_SECRET_KINDS[$key]}"
        secret_name="${ZSH_OP_SECRET_NAMES[$key]}"

        # Only load SSH keys
        [[ "$kind" == "ssh" ]] || continue

        # Load SSH key
        if _zsh_op_secret_ssh "$profile" "$secret_name" "$expiration" "$refresh"; then
            ((loaded++))
        else
            ((failed++))
        fi
    done

    if [[ $failed -gt 0 ]]; then
        gum log --level warn "Failed to load $failed SSH key(s)"
    fi

    if [[ $loaded -gt 0 ]]; then
        gum log --level info "Loaded $loaded SSH key(s) with ${expiration} expiration"
    fi

    return 0
}

# Check if SSH agent is running and has keys
_zsh_op_check_ssh_agent() {
    # ssh-add -l exit codes:
    #   0 = agent running with keys
    #   1 = agent running without keys
    #   2 = agent not running
    ssh-add -l >/dev/null 2>&1
    local status=$?
    [[ $status -ne 2 ]]
}
