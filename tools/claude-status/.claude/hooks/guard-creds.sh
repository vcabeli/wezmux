#!/usr/bin/env bash
# PreToolUse hook: block edits to the macOS keychain OAuth credential code.
# internal/creds/keychain.go handles `Claude Code-credentials` token reads;
# an accidental edit could break auth or mishandle tokens. Exit 2 blocks the
# tool call and shows the message to Claude, forcing a deliberate manual edit.
set -euo pipefail

input=$(cat)
file=$(printf '%s' "$input" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("tool_input",{}).get("file_path",""))')

case "$file" in
  *internal/creds/keychain.go)
    echo "Blocked: internal/creds/keychain.go handles macOS keychain OAuth tokens. Review and edit it manually rather than via an automated tool call." >&2
    exit 2 ;;
esac
exit 0
