#!/usr/bin/env bash
# PostToolUse hook: gofmt the edited Go file and run `go vet` on the module.
# Reads the Claude Code hook payload (JSON) on stdin; no-op for non-Go edits.
# gofmt is applied in place; vet output is advisory (never blocks the edit).
set -euo pipefail

input=$(cat)
file=$(printf '%s' "$input" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("tool_input",{}).get("file_path",""))')

case "$file" in
  *.go) ;;
  *) exit 0 ;;
esac

[ -f "$file" ] && gofmt -w "$file"

cd "${CLAUDE_PROJECT_DIR:-.}"
go vet ./... 2>&1 || echo "go vet reported issues (above) — not blocking the edit." >&2
exit 0
