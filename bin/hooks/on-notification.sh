#!/bin/bash
# wezmux hook: Claude Code Notification event
# Emits OSC 7777 structured message AND OSC 9 notification to terminal pane.
# OSC 7777 feeds the agent status store (structured status).
# OSC 9 feeds the notification store (latest_notification for sidebar preview).
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json

# Read all of stdin (Claude Code sends JSON)
input=$(cat 2>/dev/null)

. "$(dirname "$0")/lib/agent-tty.sh"
agent_tty=$(wezmux_agent_tty) || exit 0

# Extract message and type from hook input
msg=""
type=""
if command -v jq >/dev/null 2>&1; then
    msg=$(echo "$input" | jq -r '
        .message //
        .notification.message //
        .notification.body //
        .body //
        .title //
        empty
    ' 2>/dev/null)
    type=$(echo "$input" | jq -r '.notification_type // empty' 2>/dev/null)
fi

[ -z "$msg" ] && msg="Claude notification"

# Strip escape/BEL/semicolons to prevent OSC injection
msg=$(printf '%s' "$msg" | tr -d '\007\033;')

# Map the notification onto the agent's status.
#
# idle_prompt ("Claude is waiting for your input") is not a request for input:
# Claude sends it after the prompt has sat unused for a minute, with nothing
# running and no dialog open.  It means idle, and it also repairs a card left
# on Working by a turn that ended without a Stop hook (Esc interrupt).
status=""
case "$type" in
    permission_prompt | worker_permission_prompt | elicitation_dialog | \
        elicitation_url_dialog | agent_needs_input)
        status=needs_input ;;
    idle_prompt)
        status=idle ;;
    '')
        # Claude Code without notification_type: only the permission and
        # approval prompts ask for input.
        case "$msg" in
            *"needs your"* | *"permission"*) status=needs_input ;;
        esac
        ;;
esac
if [ -n "$status" ]; then
    printf '\033]7777;status;%s\007' "$status" > "$agent_tty" 2>/dev/null || true
fi

# Preview text.  Not for idle_prompt: after a turn that sent no output (an
# interrupt), the store takes the next message as the turn's output, and the
# generic "waiting for your input" would replace the last real one.
if [ "$type" != idle_prompt ]; then
    printf '\033]7777;message;%s\007' "$msg" > "$agent_tty" 2>/dev/null || true
fi
# Notification store
printf '\033]9;%s\007' "$msg" > "$agent_tty" 2>/dev/null || true
exit 0
