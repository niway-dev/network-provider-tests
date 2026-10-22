#!/usr/bin/env bash
# Runs every test script in tests/. No network access needed.
#   tests/run.sh
set -u
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fail=0
for t in "$dir"/test-*.sh; do
  echo "== $(basename "$t")"
  bash "$t" || fail=1
done
if [ "$fail" -eq 0 ]; then echo "ALL TESTS PASSED"; else echo "SOME TESTS FAILED"; fi
exit "$fail"
