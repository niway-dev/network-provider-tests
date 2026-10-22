#!/usr/bin/env bash
# shellcheck shell=bash disable=SC2034  # ROOT etc. are used by the scripts that source this
# Tiny assertion helpers shared by the test scripts. Source, do not execute.

TESTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$TESTS_DIR/.." && pwd)"
T_PASS=0
T_FAIL=0
T_NAME=""

t_ok() { T_PASS=$((T_PASS + 1)); printf '  ok    %s\n' "$T_NAME${1:+: $1}"; }
t_bad() { T_FAIL=$((T_FAIL + 1)); printf '  FAIL  %s: %s\n' "$T_NAME" "$1"; }

# assert_contains <haystack> <needle> [what]
assert_contains() {
  case "$1" in *"$2"*) t_ok "${3:-}" ;; *) t_bad "${3:-expected to contain} '$2'
--- got ---
$1
-----------" ;; esac
}
# assert_not_contains <haystack> <needle> [what]
assert_not_contains() {
  case "$1" in *"$2"*) t_bad "${3:-expected NOT to contain} '$2'
--- got ---
$1
-----------" ;; *) t_ok "${3:-}" ;; esac
}
# assert_eq <got> <expected> [what]
assert_eq() {
  if [ "$1" = "$2" ]; then t_ok "${3:-}"; else t_bad "${3:-values differ}: got '$1', expected '$2'"; fi
}

t_finish() {
  printf '%s: %d passed, %d failed\n' "$(basename "$0")" "$T_PASS" "$T_FAIL"
  [ "$T_FAIL" -eq 0 ]
}
