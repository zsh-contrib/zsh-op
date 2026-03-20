#!/usr/bin/env bats

# Tests for zsh-op lib/config.zsh
#
# Requires bats-core: https://github.com/bats-core/bats-core
# Run: bats tests/config.bats

export PLUGIN_DIR
PLUGIN_DIR="$(cd "$BATS_TEST_DIRNAME/.." && pwd)"

setup() {
  if ! command -v jq &>/dev/null; then
    skip "jq is not installed"
  fi
  if ! command -v python3 &>/dev/null; then
    skip "python3 is not installed"
  fi
  if ! python3 -c "import yaml" 2>/dev/null; then
    skip "PyYAML is not installed"
  fi
}

# Helper: load config functions with gum stubbed out
load_config_lib() {
  echo '
    gum() { :; }
    source "$PLUGIN_DIR/lib/config.zsh"
  '
}

# Helper: minimal valid config JSON
valid_json() {
  cat << 'JSON'
{
  "version": "1",
  "accounts": [
    {
      "name": "personal",
      "account": "my.1password.com",
      "secrets": [
        {"kind": "env", "name": "GITHUB_TOKEN", "path": "op://Personal/GitHub/token"},
        {"kind": "ssh", "name": "my-key",       "path": "op://Private/SSH/private key"}
      ]
    }
  ]
}
JSON
}

# ---------------------------------------------------------------------------
# _zsh_op_validate_config
# ---------------------------------------------------------------------------

@test "_zsh_op_validate_config: valid config passes" {
  run zsh -c "
    $(load_config_lib)
    json=\$(cat << 'JSON'
$(valid_json)
JSON
)
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -eq 0 ]]
}

@test "_zsh_op_validate_config: fails when version field is missing" {
  run zsh -c "
    $(load_config_lib)
    json='{\"accounts\":[{\"name\":\"p\",\"account\":\"a.1password.com\",\"secrets\":[]}]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: fails when version is not 1" {
  run zsh -c "
    $(load_config_lib)
    json='{\"version\":\"2\",\"accounts\":[{\"name\":\"p\",\"account\":\"a.1password.com\",\"secrets\":[]}]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: fails when accounts array is empty" {
  run zsh -c "
    $(load_config_lib)
    json='{\"version\":\"1\",\"accounts\":[]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: fails when account name is missing" {
  run zsh -c "
    $(load_config_lib)
    json='{\"version\":\"1\",\"accounts\":[{\"account\":\"a.1password.com\",\"secrets\":[]}]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: fails when account url is missing" {
  run zsh -c "
    $(load_config_lib)
    json='{\"version\":\"1\",\"accounts\":[{\"name\":\"personal\",\"secrets\":[]}]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: fails when secret kind is invalid" {
  run zsh -c "
    $(load_config_lib)
    json='{\"version\":\"1\",\"accounts\":[{\"name\":\"p\",\"account\":\"a.1password.com\",\"secrets\":[{\"kind\":\"token\",\"name\":\"TOK\",\"path\":\"op://V/I/F\"}]}]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: fails when secret path does not start with op://" {
  run zsh -c "
    $(load_config_lib)
    json='{\"version\":\"1\",\"accounts\":[{\"name\":\"p\",\"account\":\"a.1password.com\",\"secrets\":[{\"kind\":\"env\",\"name\":\"TOK\",\"path\":\"vault://Personal/GitHub/token\"}]}]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: fails when secret name is missing" {
  run zsh -c "
    $(load_config_lib)
    json='{\"version\":\"1\",\"accounts\":[{\"name\":\"p\",\"account\":\"a.1password.com\",\"secrets\":[{\"kind\":\"env\",\"path\":\"op://V/I/F\"}]}]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: fails when secret path is missing" {
  run zsh -c "
    $(load_config_lib)
    json='{\"version\":\"1\",\"accounts\":[{\"name\":\"p\",\"account\":\"a.1password.com\",\"secrets\":[{\"kind\":\"env\",\"name\":\"TOK\"}]}]}'
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -ne 0 ]]
}

@test "_zsh_op_validate_config: accepts both env and ssh secret kinds" {
  run zsh -c "
    $(load_config_lib)
    json=\$(cat << 'JSON'
$(valid_json)
JSON
)
    _zsh_op_validate_config \"\$json\"
  "
  [[ "$status" -eq 0 ]]
}

# ---------------------------------------------------------------------------
# _zsh_op_parse_yaml
# ---------------------------------------------------------------------------

@test "_zsh_op_parse_yaml: parses valid YAML into JSON" {
  local config_file
  config_file="$BATS_TEST_TMPDIR/config.yml"
  cat > "$config_file" << 'YAML'
version: 1
accounts:
  - name: personal
    account: my.1password.com
    secrets:
      - kind: env
        name: GITHUB_TOKEN
        path: op://Personal/GitHub/token
YAML

  run zsh -c "
    $(load_config_lib)
    _zsh_op_parse_yaml '$config_file'
  "
  [[ "$status" -eq 0 ]]
  # Output should be valid JSON
  echo "$output" | jq . > /dev/null
  [[ "${PIPESTATUS[1]}" -eq 0 ]]
}

@test "_zsh_op_parse_yaml: parsed JSON contains account name" {
  local config_file
  config_file="$BATS_TEST_TMPDIR/config.yml"
  cat > "$config_file" << 'YAML'
version: 1
accounts:
  - name: personal
    account: my.1password.com
    secrets: []
YAML

  run zsh -c "
    $(load_config_lib)
    _zsh_op_parse_yaml '$config_file' | jq -r '.accounts[0].name'
  "
  [[ "$status" -eq 0 ]]
  [[ "$output" == "personal" ]]
}

@test "_zsh_op_parse_yaml: fails when config file does not exist" {
  run zsh -c "
    $(load_config_lib)
    _zsh_op_parse_yaml '/nonexistent/config.yml'
  "
  [[ "$status" -ne 0 ]]
}

# ---------------------------------------------------------------------------
# _zsh_op_profile_exists / _zsh_op_secret_exists
# ---------------------------------------------------------------------------

@test "_zsh_op_profile_exists: returns 0 for a loaded profile" {
  local config_file
  config_file="$BATS_TEST_TMPDIR/config.yml"
  cat > "$config_file" << 'YAML'
version: 1
accounts:
  - name: personal
    account: my.1password.com
    secrets: []
YAML

  run zsh -c "
    $(load_config_lib)
    typeset -gA ZSH_OP_ACCOUNTS ZSH_OP_SECRETS ZSH_OP_SECRET_KINDS ZSH_OP_SECRET_NAMES
    _zsh_op_load_config '$config_file'
    _zsh_op_profile_exists personal && echo 'found' || echo 'not found'
  "
  [[ "$status" -eq 0 ]]
  [[ "$output" == "found" ]]
}

@test "_zsh_op_profile_exists: returns 1 for a missing profile" {
  run zsh -c "
    $(load_config_lib)
    typeset -gA ZSH_OP_ACCOUNTS ZSH_OP_SECRETS ZSH_OP_SECRET_KINDS ZSH_OP_SECRET_NAMES
    ZSH_OP_ACCOUNTS=()
    _zsh_op_profile_exists nonexistent && echo 'found' || echo 'not found'
  "
  [[ "$status" -eq 0 ]]
  [[ "$output" == "not found" ]]
}

@test "_zsh_op_secret_exists: returns 0 for a loaded secret" {
  local config_file
  config_file="$BATS_TEST_TMPDIR/config.yml"
  cat > "$config_file" << 'YAML'
version: 1
accounts:
  - name: personal
    account: my.1password.com
    secrets:
      - kind: env
        name: GITHUB_TOKEN
        path: op://Personal/GitHub/token
YAML

  run zsh -c "
    $(load_config_lib)
    typeset -gA ZSH_OP_ACCOUNTS ZSH_OP_SECRETS ZSH_OP_SECRET_KINDS ZSH_OP_SECRET_NAMES
    _zsh_op_load_config '$config_file'
    _zsh_op_secret_exists personal GITHUB_TOKEN && echo 'found' || echo 'not found'
  "
  [[ "$status" -eq 0 ]]
  [[ "$output" == "found" ]]
}

@test "_zsh_op_secret_exists: returns 1 for a missing secret" {
  run zsh -c "
    $(load_config_lib)
    typeset -gA ZSH_OP_ACCOUNTS ZSH_OP_SECRETS ZSH_OP_SECRET_KINDS ZSH_OP_SECRET_NAMES
    ZSH_OP_SECRETS=()
    _zsh_op_secret_exists personal NONEXISTENT && echo 'found' || echo 'not found'
  "
  [[ "$status" -eq 0 ]]
  [[ "$output" == "not found" ]]
}

@test "_zsh_op_get_secrets: lists secrets for a profile" {
  local config_file
  config_file="$BATS_TEST_TMPDIR/config.yml"
  cat > "$config_file" << 'YAML'
version: 1
accounts:
  - name: personal
    account: my.1password.com
    secrets:
      - kind: env
        name: GITHUB_TOKEN
        path: op://Personal/GitHub/token
      - kind: ssh
        name: my-key
        path: op://Private/SSH/key
YAML

  run zsh -c "
    $(load_config_lib)
    typeset -gA ZSH_OP_ACCOUNTS ZSH_OP_SECRETS ZSH_OP_SECRET_KINDS ZSH_OP_SECRET_NAMES
    _zsh_op_load_config '$config_file'
    secrets=(\$(_zsh_op_get_secrets personal))
    echo \${#secrets[@]}
  "
  [[ "$status" -eq 0 ]]
  [[ "$output" -eq 2 ]]
}
