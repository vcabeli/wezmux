#!/bin/bash
# wezmux hook: Codex SessionStart event
# Tells Codex it can draw inline diagrams, through the hook's
# additionalContext (bin/hooks/lib/diagram-prompt.sh).
#
# Only the environment says whether this session runs inside Wezmux. A Codex
# attached to the app-server daemon runs its hooks with the environment of
# the pane that started the daemon, so a daemon started in Wezmux tells every
# session it serves.

[ "${WEZMUX:-}" = 1 ] && [ "${WEZMUX_DIAGRAMS:-1}" = 1 ] || exit 0
command -v jq >/dev/null 2>&1 || exit 0
. "$(dirname "$0")/../lib/diagram-prompt.sh"

jq -nc --arg context "$WEZMUX_DIAGRAM_PROMPT" \
    '{hookSpecificOutput: {hookEventName: "SessionStart", additionalContext: $context}}'
exit 0
