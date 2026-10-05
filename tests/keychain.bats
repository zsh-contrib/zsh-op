#!/usr/bin/env bats

# Tests for zsh-op lib/keychain.zsh

export PLUGIN_DIR
PLUGIN_DIR="$(cd "$BATS_TEST_DIRNAME/.." && pwd)"

setup() {
  if ! command -v zsh &>/dev/null; then
    skip "zsh is not installed"
  fi
}

# Stubs security find-generic-password -g with the given password line, as
# macOS prints it to stderr.
load_keychain_lib() {
  echo "
    gum() { :; }
    source \"\$PLUGIN_DIR/lib/keychain.zsh\"
    security() {
      print -r -- 'keychain: \"/Users/test/Library/Keychains/login.keychain-db\"'
      print -r -u2 -- '$1'
    }
  "
}

@test "_zsh_op_keychain_read: returns a 64-character hex secret as is" {
  run zsh -c "
    $(load_keychain_lib 'password: "da53422a4618735f8ae72abdb21c16d340849b1a510ef20768b131171f8f7122"')
    _zsh_op_keychain_read op-secrets-personal R2_SECRET_ACCESS_KEY
  "

  [[ "$status" -eq 0 ]]
  [[ "$output" == 'da53422a4618735f8ae72abdb21c16d340849b1a510ef20768b131171f8f7122' ]]
}

@test "_zsh_op_keychain_read: decodes a hex-encoded multiline value" {
  run zsh -c "
    $(load_keychain_lib 'password: 0x6C696E65206F6E650A6C696E652074776F  "line one\012line two"')
    _zsh_op_keychain_read op-secrets-personal MULTILINE
  "

  [[ "$status" -eq 0 ]]
  [[ "${lines[0]}" == 'line one' ]]
  [[ "${lines[1]}" == 'line two' ]]
}

@test "_zsh_op_keychain_read: decodes a short hex-encoded non-ASCII value" {
  run zsh -c "
    $(load_keychain_lib 'password: 0x70C3A4737377C3B67264  "p\303\244ssw\303\266rd"')
    _zsh_op_keychain_read op-secrets-personal UTF8
  "

  [[ "$status" -eq 0 ]]
  [[ "$output" == 'pässwörd' ]]
}

@test "_zsh_op_keychain_read: keeps backslashes in a hex-encoded value" {
  run zsh -c "
    $(load_keychain_lib 'password: 0x615C6E62  "a\134nb"')
    _zsh_op_keychain_read op-secrets-personal BACKSLASH
  "

  [[ "$status" -eq 0 ]]
  [[ "$output" == 'a\nb' ]]
}

@test "_zsh_op_keychain_read: returns a value with quotes and spaces as is" {
  run zsh -c "
    $(load_keychain_lib 'password: "  {"type": "service_account"}  "')
    _zsh_op_keychain_read op-secrets-personal JSON
  "

  [[ "$status" -eq 0 ]]
  [[ "$output" == '  {"type": "service_account"}  ' ]]
}

@test "_zsh_op_keychain_read: fails when the item isn't in the keychain" {
  run zsh -c "
    gum() { :; }
    source \"\$PLUGIN_DIR/lib/keychain.zsh\"
    security() {
      print -r -u2 -- 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.'
      return 44
    }
    _zsh_op_keychain_read op-secrets-personal MISSING
  "

  [[ "$status" -eq 1 ]]
  [[ -z "$output" ]]
}
