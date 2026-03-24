#!/bin/bash
# Install wezmux Claude Code hooks
# Copies hook scripts to ~/.claude/hooks/wezmux/ and shows settings.json snippet
set -e

DEST="$HOME/.claude/hooks/wezmux"
SRC="$(cd "$(dirname "$0")/hooks" && pwd)"

mkdir -p "$DEST"
cp "$SRC"/on-notification.sh "$DEST/"
cp "$SRC"/on-stop.sh "$DEST/"
cp "$SRC"/on-prompt-submit.sh "$DEST/"
cp "$SRC"/on-needs-input.sh "$DEST/"
chmod +x "$DEST"/*.sh

echo "Installed hooks to $DEST"
echo ""
echo "Add to ~/.claude/settings.json 'hooks' section:"
cat <<'EOF'

"Notification": [
  { "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux/on-notification.sh", "timeout": 5 }] }
],
"Stop": [
  { "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux/on-stop.sh", "timeout": 5 }] }
],
"UserPromptSubmit": [
  { "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux/on-prompt-submit.sh", "timeout": 5 }] }
],
"PreToolUse": [
  ...,
  { "matcher": "AskUserQuestion", "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux/on-needs-input.sh", "timeout": 5 }] }
]
EOF
