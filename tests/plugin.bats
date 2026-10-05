#!/usr/bin/env bats

# Tests for zsh-op.plugin.zsh
#
# The secret-env binary is replaced by a stub that records the arguments and
# environment it receives, and prints $STUB_OUTPUT.
#
# Requires bats-core: https://github.com/bats-core/bats-core
# Run: bats tests/

bats_require_minimum_version 1.5.0

export PLUGIN_DIR
PLUGIN_DIR="$(cd "$BATS_TEST_DIRNAME/.." && pwd)"

setup() {
  export STUB="$BATS_TEST_TMPDIR/secret-env"
  export STUB_LOG="$BATS_TEST_TMPDIR/calls.log"
  cat > "$STUB" <<'EOF'
#!/bin/sh
{
  printf 'args: %s\n' "$*"
  printf 'env: config=%s cache=%s profile=%s\n' \
    "$SECRET_ENV_CONFIG_FILE" "$SECRET_ENV_CACHE_DIR" "$SECRET_ENV_DEFAULT_PROFILE"
} >> "$STUB_LOG"
[ -n "$STUB_OUTPUT" ] && printf '%s\n' "$STUB_OUTPUT"
exit "${STUB_RC:-0}"
EOF
  chmod +x "$STUB"

  export ZSH_OP_BIN="$STUB"
  export ZSH_OP_CONFIG_FILE="$BATS_TEST_TMPDIR/config.yml"
  export ZSH_OP_CACHE_DIR="$BATS_TEST_TMPDIR/cache"
  touch "$ZSH_OP_CONFIG_FILE"
}

# Run a zsh snippet with the plugin loaded. Extra arguments go to `run`
# before the command, e.g. `zsh_op 'op-shell' -127`.
zsh_op() {
  run "${@:2}" zsh -f -c "source '$PLUGIN_DIR/zsh-op.plugin.zsh'; $1"
}

@test "plugin: auto-exports cached secrets of loaded profiles on load" {
  STUB_OUTPUT="export ZSH_OP_TEST_TOKEN='brown-fox'" zsh_op 'print -r -- "$ZSH_OP_TEST_TOKEN"'
  [[ "$status" -eq 0 ]]
  [[ "$output" == "brown-fox" ]]
  grep -q "^args: export --all --cached --format zsh --runtime-dir " "$STUB_LOG"
}

@test "plugin: auto-export can be disabled" {
  ZSH_OP_AUTO_EXPORT=false STUB_OUTPUT="export ZSH_OP_TEST_TOKEN='brown-fox'" \
    zsh_op 'print -r -- "${ZSH_OP_TEST_TOKEN-unset}"'
  [[ "$output" == "unset" ]]
  [[ ! -e "$STUB_LOG" ]]
}

@test "plugin: auto-export is skipped without a config file" {
  rm "$ZSH_OP_CONFIG_FILE"
  zsh_op 'true'
  [[ "$status" -eq 0 ]]
  [[ ! -e "$STUB_LOG" ]]
}

@test "plugin: loads silently when secret-env is missing" {
  ZSH_OP_BIN="$BATS_TEST_TMPDIR/missing" zsh_op 'true'
  [[ "$status" -eq 0 ]]
  [[ -z "$output" ]]
}

@test "plugin: passes the zsh-op settings to secret-env" {
  ZSH_OP_AUTO_EXPORT=false ZSH_OP_DEFAULT_PROFILE=work zsh_op 'op-secret A'
  grep -qx "env: config=$ZSH_OP_CONFIG_FILE cache=$ZSH_OP_CACHE_DIR profile=work" "$STUB_LOG"
}

@test "plugin: removes the runtime directory when the shell exits" {
  ZSH_OP_AUTO_EXPORT=false zsh_op 'op-secret A >/dev/null; print -r -- "$_ZSH_OP_RUNTIME_DIR"'
  [[ -n "$output" ]]
  [[ ! -e "$output" ]]
}

@test "op-shell: passes options through and evaluates the exports" {
  ZSH_OP_AUTO_EXPORT=false STUB_OUTPUT="export API_KEY='it'\''s'" \
    zsh_op 'op-shell work -e 8h -r; print -r -- "rc=$? API_KEY=$API_KEY"'
  [[ "$output" == "rc=0 API_KEY=it's" ]]
  grep -Eq "^args: shell --format zsh --runtime-dir [^ ]+ work -e 8h -r$" "$STUB_LOG"
}

@test "op-shell: applies the exports and keeps the exit status on partial failure" {
  ZSH_OP_AUTO_EXPORT=false STUB_RC=1 STUB_OUTPUT="export A='1'" \
    zsh_op 'op-shell; print -r -- "rc=$? A=$A"'
  [[ "$output" == "rc=1 A=1" ]]
}

@test "op-shell: shows help without running secret-env" {
  ZSH_OP_AUTO_EXPORT=false zsh_op 'op-shell -h'
  [[ "$status" -eq 0 ]]
  [[ "$output" == Usage:\ op-shell* ]]
  [[ ! -e "$STUB_LOG" ]]
}

@test "op-shell: reports a missing secret-env binary" {
  ZSH_OP_BIN="$BATS_TEST_TMPDIR/missing" zsh_op 'op-shell' -127
  [[ "$output" == *"install secret-env"* ]]
}

@test "op-secret: prints the value without evaluating it" {
  ZSH_OP_AUTO_EXPORT=false STUB_OUTPUT='export PWNED=1' \
    zsh_op 'op-secret -p work API_KEY; print -r -- "PWNED=${PWNED-unset}"'
  [[ "${lines[0]}" == "export PWNED=1" ]]
  [[ "${lines[1]}" == "PWNED=unset" ]]
  grep -Eq "^args: secret --runtime-dir [^ ]+ -p work API_KEY$" "$STUB_LOG"
}

@test "op-secret: -x evaluates the export statement" {
  ZSH_OP_AUTO_EXPORT=false STUB_OUTPUT="export API_KEY='v'" \
    zsh_op 'op-secret API_KEY -x; print -r -- "rc=$? API_KEY=$API_KEY"'
  [[ "$output" == "rc=0 API_KEY=v" ]]
  grep -Eq "^args: secret --export --format zsh --runtime-dir [^ ]+ API_KEY$" "$STUB_LOG"
}

@test "op-secret: shows help without running secret-env" {
  ZSH_OP_AUTO_EXPORT=false zsh_op 'op-secret --help'
  [[ "$output" == Usage:\ op-secret* ]]
  [[ ! -e "$STUB_LOG" ]]
}

@test "completions: compinit registers op-shell and op-secret" {
  ZSH_OP_AUTO_EXPORT=false zsh_op "autoload -Uz compinit; compinit -u -d '$BATS_TEST_TMPDIR/zcompdump'; print -r -- \"\${_comps[op-shell]} \${_comps[op-secret]}\""
  [[ "$output" == "_op_shell _op_secret" ]]
}
