#!/usr/bin/env bash
# Checks that FEATURES.md and the engine's test suite agree:
#   - every test named in FEATURES.md exists
#   - every test exists in FEATURES.md (so nothing is untracked)
set -euo pipefail
cd "$(dirname "$0")/.."

tests=$(cargo test -q -p procalc-engine -- --list 2>/dev/null | sed -n 's/: test$//p' | sort -u)
mentioned=$(grep -oE '`[a-z0-9_]+(::[a-z0-9_]+)+`' FEATURES.md | tr -d '`' | sort -u)

missing=$(comm -23 <(echo "$mentioned") <(echo "$tests"))
untracked=$(comm -13 <(echo "$mentioned") <(echo "$tests"))

status=0
if [[ -n "$missing" ]]; then
  echo "Named in FEATURES.md but no such test:"; echo "$missing" | sed 's/^/  /'; status=1
fi
if [[ -n "$untracked" ]]; then
  echo "Tests not listed in FEATURES.md:"; echo "$untracked" | sed 's/^/  /'; status=1
fi
[[ $status -eq 0 ]] && echo "FEATURES.md and tests match ($(echo "$tests" | wc -l) tests)."
exit $status
