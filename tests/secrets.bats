#!/usr/bin/env bats

# Tests for zsh-op lib/secrets.zsh

export PLUGIN_DIR
PLUGIN_DIR="$(cd "$BATS_TEST_DIRNAME/.." && pwd)"

setup() {
  if ! command -v zsh &>/dev/null; then
    skip "zsh is not installed"
  fi
}

load_secrets_lib() {
  echo '
    gum() { :; }
    source "$PLUGIN_DIR/lib/secrets.zsh"
  '
}

@test "_zsh_op_export_cached_secrets: current file kind overrides stale env metadata" {
  run zsh -c "
    $(load_secrets_lib)
    typeset -gA ZSH_OP_SECRET_KINDS
    ZSH_OP_CACHE_DIR='$BATS_TEST_TMPDIR/cache'
    mkdir -p \"\$ZSH_OP_CACHE_DIR\"
    printf '%s\n' 'env:GOOGLE_APPLICATION_CREDENTIALS' > \"\$ZSH_OP_CACHE_DIR/personal.metadata\"
    ZSH_OP_SECRET_KINDS[personal:GOOGLE_APPLICATION_CREDENTIALS]=file
    _zsh_op_keychain_read() { print -r -- '{\"type\":\"service_account\"}'; }

    _zsh_op_export_cached_secrets personal
    print -r -- \"\$GOOGLE_APPLICATION_CREDENTIALS\"
    print -r -- \"\$(<\"\$GOOGLE_APPLICATION_CREDENTIALS\")\"
  "

  [[ "$status" -eq 0 ]]
  [[ "${lines[0]}" == "$BATS_TEST_TMPDIR/cache/files/personal/GOOGLE_APPLICATION_CREDENTIALS" ]]
  [[ "${lines[1]}" == '{"type":"service_account"}' ]]
}
