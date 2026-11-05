#!/bin/bash
# wezmux hook: Claude Code PostToolBatch event
# Reports Working once a batch of tool calls has finished.
#
# Nothing else tells the sidebar the user answered: approving a permission
# prompt or an AskUserQuestion dialog fires no UserPromptSubmit, so the card
# stayed on "Needs input" while Claude carried on, until the turn's Stop.
# The tool batch completing means Claude is running again.  While it already
# shows Working, the store treats the repeat as a no-op.

. "$(dirname "$0")/lib/agent-tty.sh"
agent_tty=$(wezmux_agent_tty) || exit 0

printf '\033]7777;status;working\007' > "$agent_tty" 2>/dev/null || true
exit 0
