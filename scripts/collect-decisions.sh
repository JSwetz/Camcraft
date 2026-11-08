#!/usr/bin/env bash
# Gather docs/decisions/*.md into RECOMMENDED.md at a release cut (one file
# per PR keeps issue branches from conflicting on RECOMMENDED.md).
# Usage: scripts/collect-decisions.sh 0.14.0
set -euo pipefail
version="${1:?usage: collect-decisions.sh <version>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
dir="$root/docs/decisions"
files=$(cd "$dir" && ls *.md 2>/dev/null | grep -v '^README.md$' | sort -t- -k1,1n -k2 | sed "s#^#$dir/#" || true)
[ -z "$files" ] && { echo "no decision files to collect"; exit 0; }
{ printf '\n## v%s - decisions gathered at the cut\n' "$version"
  for f in $files; do printf '\n'; cat "$f"; printf '\n'; done; } >> "$root/RECOMMENDED.md"
n=0; for f in $files; do rm -- "$f"; n=$((n+1)); done
echo "collected $n decision file(s) into RECOMMENDED.md"
