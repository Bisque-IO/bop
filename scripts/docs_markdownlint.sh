#!/usr/bin/env bash

set -euo pipefail

if command -v markdownlint >/dev/null 2>&1; then
  LINT_BIN=(markdownlint)
elif command -v npx >/dev/null 2>&1; then
  LINT_BIN=(npx --yes markdownlint-cli@0.39.0)
else
  echo "Install Node.js or provide markdownlint on PATH" >&2
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

shopt -s globstar nullglob
files=(docs/**/*.md)
if [ ${#files[@]} -eq 0 ]; then
  echo "No markdown files found under docs/" >&2
  exit 1
fi

"${LINT_BIN[@]}" --config docs/.markdownlint.json "${files[@]}"
