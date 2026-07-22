#!/usr/bin/env bash
# Deterministic Brotli-transfer size gate for the Flutter Web build.
#
# For each budget in perf-budgets.json, sum the Brotli(q11) size of the matched
# files and fail (exit 1) if it exceeds maxBrotliKB. This measures the size a
# server WOULD transfer with Content-Encoding: br, so it is independent of
# whether the running server actually compresses -- a reproducible gate.
#
# Deps: brotli, jq (both on ubuntu-latest CI after `apt-get install -y brotli`).
# Usage: tool/size_budget.sh [BUILD_DIR] [BUDGETS_JSON]
#   BUILD_DIR    default: build/web
#   BUDGETS_JSON default: perf-budgets.json
set -euo pipefail
shopt -s globstar nullglob

WEB="${1:-build/web}"
BUDGETS="${2:-perf-budgets.json}"

command -v brotli >/dev/null || { echo "error: 'brotli' not found (apt-get install -y brotli)"; exit 2; }
command -v jq     >/dev/null || { echo "error: 'jq' not found"; exit 2; }
[ -d "$WEB" ]     || { echo "error: build dir '$WEB' not found (run flutter build web --release)"; exit 2; }
[ -f "$BUDGETS" ] || { echo "error: budgets file '$BUDGETS' not found"; exit 2; }

br_bytes() { brotli -q 11 -c -- "$1" | wc -c; }

n=$(jq '.budgets | length' "$BUDGETS")
fails=0

printf '%-48s %10s %10s   %s\n' "BUDGET" "BROTLI" "LIMIT" "RESULT"
printf '%s\n' "--------------------------------------------------------------------------------"

for i in $(seq 0 $((n - 1))); do
  label=$(jq -r ".budgets[$i].label" "$BUDGETS")
  max=$(jq -r ".budgets[$i].maxBrotliKB" "$BUDGETS")

  # Expand globs (relative to $WEB), de-duplicate files.
  unset seen; declare -A seen
  files=()
  while IFS= read -r g; do
    for f in "$WEB"/$g; do
      [ -f "$f" ] || continue
      [ -n "${seen[$f]:-}" ] && continue
      seen[$f]=1
      files+=("$f")
    done
  done < <(jq -r ".budgets[$i].globs[]" "$BUDGETS")

  total=0
  for f in "${files[@]}"; do
    total=$((total + $(br_bytes "$f")))
  done

  # Compare in awk (float KB) and format.
  if awk -v t="$total" -v m="$max" 'BEGIN{ exit ((t/1024) > m) ? 1 : 0 }'; then
    result="PASS"
  else
    result="FAIL"
    fails=$((fails + 1))
  fi
  kb=$(awk -v t="$total" 'BEGIN{ printf "%.1f", t/1024 }')
  printf '%-48s %9s %9s   %s\n' "${label:0:48}" "${kb} KB" "${max} KB" "$result"
done

printf '%s\n' "--------------------------------------------------------------------------------"
if [ "$fails" -gt 0 ]; then
  echo "FAILED: $fails budget(s) exceeded. See docs/web-performance.md to adjust."
  exit 1
fi
echo "OK: all $n Brotli budgets within limit."
