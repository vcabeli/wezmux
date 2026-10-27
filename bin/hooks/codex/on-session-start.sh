#!/bin/bash
# wezmux hook: Codex SessionStart event
# Tells Codex it can draw inline diagrams and math, through the hook's
# additionalContext (bin/hooks/lib/inline-prompt.sh).
#
# Only the environment says whether this session runs inside Wezmux. A Codex
# attached to the app-server daemon runs its hooks with the environment of
# the pane that started the daemon, so a daemon started in Wezmux tells every
# session it serves.

[ "${WEZMUX:-}" = 1 ] || exit 0
command -v jq >/dev/null 2>&1 || exit 0
. "$(dirname "$0")/../lib/inline-prompt.sh"
[ -n "$WEZMUX_INLINE_PROMPT" ] || exit 0

jq -nc --arg context "$WEZMUX_INLINE_PROMPT" \
    '{hookSpecificOutput: {hookEventName: "SessionStart", additionalContext: $context}}'
exit 0
