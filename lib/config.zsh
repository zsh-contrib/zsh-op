#!/usr/bin/env zsh
# lib/config.zsh - Configuration parsing and validation

# Check if all required dependencies are available (zsh-native way)
_zsh_op_check_dependencies() {
    local missing=()

    # Use zsh's built-in commands array to find and cache command paths
    (( $+commands[gum] )) && ZSH_OP_GUM=$commands[gum] || missing+=("gum")
    (( $+commands[op] )) && ZSH_OP_OP=$commands[op] || missing+=("op")
    (( $+commands[security] )) && ZSH_OP_SECURITY=$commands[security] || missing+=("security")
    (( $+commands[python3] )) && ZSH_OP_PYTHON3=$commands[python3] || missing+=("python3")
    (( $+commands[jq] )) && ZSH_OP_JQ=$commands[jq] || missing+=("jq")

    if [[ ${#missing[@]} -gt 0 ]]; then
        ${ZSH_OP_GUM:-echo} log --level error "Missing required dependencies: ${missing[*]}" 2>/dev/null || \
            echo "ERROR: Missing required dependencies: ${missing[*]}" >&2
        return 1
    fi

    # Check for PyYAML module
    if ! $ZSH_OP_PYTHON3 -c "import yaml" 2>/dev/null; then
        ${ZSH_OP_GUM:-echo} log --level error "PyYAML module is required but not found" 2>/dev/null || \
            echo "ERROR: PyYAML module is required but not found" >&2
        ${ZSH_OP_GUM:-echo} log --level info "Install with: pip3 install PyYAML" 2>/dev/null || \
            echo "INFO: Install with: pip3 install PyYAML" >&2
        return 1
    fi

    return 0
}

# Parse YAML config file to JSON using $ZSH_OP_PYTHON3
_zsh_op_parse_yaml() {
    local config_file="$1"

    if [[ ! -f "$config_file" ]]; then
        gum log --level error "Config file not found: $config_file"
        gum log --level info "Create one at: $config_file"
        gum log --level info "See: ${ZSH_OP_PLUGIN_DIR}/config.example.yml"
        return 1
    fi

    # Convert YAML to JSON using $ZSH_OP_PYTHON3
    local json
    if ! json=$($ZSH_OP_PYTHON3 -c "import yaml, json, sys; print(json.dumps(yaml.safe_load(sys.stdin)))" < "$config_file" 2>&1); then
        gum log --level error "Failed to parse config file"
        gum log --level error "YAML parsing error: $json"
        return 1
    fi

    echo "$json"
}

# Validate config structure and content
_zsh_op_validate_config() {
    local json="$1"

    # Check version field
    local version
    version=$(echo "$json" | $ZSH_OP_JQ -r '.version // empty')
    if [[ -z "$version" ]]; then
        gum log --level error "Config missing 'version' field"
        return 1
    fi

    if [[ "$version" != "1" ]]; then
        gum log --level error "Unsupported config version: $version"
        gum log --level info "Supported versions: 1"
        return 1
    fi

    # Check accounts array exists
    local accounts_count
    accounts_count=$(echo "$json" | $ZSH_OP_JQ '.accounts | length')
    if [[ "$accounts_count" -eq 0 ]]; then
        gum log --level error "Config has no accounts defined"
        return 1
    fi

    # Validate each account
    local i=0
    while [[ $i -lt $accounts_count ]]; do
        local account_name account_url secrets_count
        account_name=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].name // empty")
        account_url=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].account // empty")
        secrets_count=$(echo "$json" | $ZSH_OP_JQ ".accounts[$i].secrets | length")

        # Check required fields
        if [[ -z "$account_name" ]]; then
            gum log --level error "Account at index $i missing 'name' field"
            return 1
        fi

        if [[ -z "$account_url" ]]; then
            gum log --level error "Account '$account_name' missing 'account' field"
            return 1
        fi

        # Validate secrets
        local j=0
        while [[ $j -lt $secrets_count ]]; do
            local secret_kind secret_name secret_path
            secret_kind=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].secrets[$j].kind // empty")
            secret_name=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].secrets[$j].name // empty")
            secret_path=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].secrets[$j].path // empty")

            # Check required fields
            if [[ -z "$secret_kind" ]]; then
                gum log --level error "Secret at account '$account_name' index $j missing 'kind' field"
                return 1
            fi

            if [[ "$secret_kind" != "env" && "$secret_kind" != "ssh" ]]; then
                gum log --level error "Secret '$secret_name' has invalid kind: $secret_kind"
                gum log --level info "Valid kinds: env, ssh"
                return 1
            fi

            if [[ -z "$secret_name" ]]; then
                gum log --level error "Secret at account '$account_name' index $j missing 'name' field"
                return 1
            fi

            if [[ -z "$secret_path" ]]; then
                gum log --level error "Secret '$secret_name' in account '$account_name' missing 'path' field"
                return 1
            fi

            # Validate op:// path format
            if [[ ! "$secret_path" =~ ^op:// ]]; then
                gum log --level error "Secret '$secret_name' has invalid path: $secret_path"
                gum log --level info "Path must start with 'op://'"
                return 1
            fi

            ((j++))
        done

        ((i++))
    done

    return 0
}

# Load config into global associative arrays
_zsh_op_load_config() {
    # Disable xtrace for clean config loading
    setopt local_options no_xtrace

    local config_file="${1:-$ZSH_OP_CONFIG_FILE}"

    # Check dependencies first
    _zsh_op_check_dependencies || return 1

    # Parse YAML to JSON
    local json
    json=$(_zsh_op_parse_yaml "$config_file") || return 1

    # Validate config
    _zsh_op_validate_config "$json" || return 1

    # Clear existing config
    ZSH_OP_ACCOUNTS=()
    ZSH_OP_SECRETS=()
    ZSH_OP_SECRET_KINDS=()
    ZSH_OP_SECRET_NAMES=()

    # Load accounts and secrets
    local accounts_count
    accounts_count=$(echo "$json" | $ZSH_OP_JQ '.accounts | length')

    local i=0
    while [[ $i -lt $accounts_count ]]; do
        local profile account_url
        profile=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].name")
        account_url=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].account")

        # Store account mapping
        ZSH_OP_ACCOUNTS[$profile]="$account_url"

        # Load secrets for this account
        local secrets_count
        secrets_count=$(echo "$json" | $ZSH_OP_JQ ".accounts[$i].secrets | length")

        local j=0
        while [[ $j -lt $secrets_count ]]; do
            local kind name path
            kind=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].secrets[$j].kind")
            name=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].secrets[$j].name")
            path=$(echo "$json" | $ZSH_OP_JQ -r ".accounts[$i].secrets[$j].path")

            # Store in global arrays with composite key: profile:name
            local key="${profile}:${name}"
            ZSH_OP_SECRETS[$key]="$path"
            ZSH_OP_SECRET_KINDS[$key]="$kind"
            ZSH_OP_SECRET_NAMES[$key]="$name"

            ((j++))
        done

        ((i++))
    done

    return 0
}

# Get list of profiles from config
_zsh_op_get_profiles() {
    echo "${(@k)ZSH_OP_ACCOUNTS}"
}

# Get list of secret names for a profile
_zsh_op_get_secrets() {
    local profile="$1"
    local secrets=()

    for key in ${(k)ZSH_OP_SECRETS}; do
        if [[ "$key" =~ ^${profile}: ]]; then
            secrets+=(${key#*:})
        fi
    done

    echo "${secrets[@]}"
}

# Check if profile exists
_zsh_op_profile_exists() {
    local profile="$1"
    [[ -n "${ZSH_OP_ACCOUNTS[$profile]}" ]]
}

# Check if secret exists for profile
_zsh_op_secret_exists() {
    local profile="$1"
    local secret_name="$2"
    local key="${profile}:${secret_name}"
    [[ -n "${ZSH_OP_SECRETS[$key]}" ]]
}
