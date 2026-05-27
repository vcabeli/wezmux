#!/bin/bash
# wezmux hook: Claude Code Stop event
# Emits OSC 7777 structured status AND OSC 9 notification to terminal pane.
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json

input=$(cat 2>/dev/null)

reason="end_turn"
last_message=""
bg_task_count=""
cron_count=""
if command -v jq >/dev/null 2>&1; then
    reason=$(echo "$input" | jq -r '.stop_hook_reason // .reason // "end_turn"' 2>/dev/null)
    last_message=$(echo "$input" | jq -r '.last_assistant_message // empty' 2>/dev/null)
    # background_tasks / session_crons arrays (Claude Code v2.1.145+).
    # Empty string when the field is absent so we can skip the OSC emit
    # and remain compatible with older Claude Code releases.
    bg_task_count=$(echo "$input" | jq -r '(.background_tasks // []) | length' 2>/dev/null)
    cron_count=$(echo "$input" | jq -r '(.session_crons // []) | length' 2>/dev/null)
fi

case "$reason" in
    end_turn)       msg="Claude finished" ;;
    stop_button)    msg="Claude stopped by user" ;;
    interrupt)      msg="Claude interrupted" ;;
    *)              msg="Claude finished ($reason)" ;;
esac

# Use Claude's actual response as preview, truncated to ~200 chars
preview="$msg"
if [ -n "$last_message" ]; then
    # Take first 200 chars, collapse whitespace
    preview=$(echo "$last_message" | tr '\n' ' ' | sed 's/  */ /g' | cut -c1-200)
fi

# Strip escape/BEL/semicolons to prevent OSC injection
preview=$(printf '%s' "$preview" | tr -d '\007\033;')

# Structured status for agent store
printf '\033]7777;status;idle\007' > "${WEZMUX_TTY:-/dev/tty}" 2>/dev/null || true
printf '\033]7777;message;%s\007' "$preview" > "${WEZMUX_TTY:-/dev/tty}" 2>/dev/null || true
# Background tasks & scheduled tasks: authoritative count from the harness.
# Only emit when jq could parse the field (older Claude versions omit it).
case "$bg_task_count" in
    ''|*[!0-9]*) ;;
    *) printf '\033]7777;background_tasks;%s\007' "$bg_task_count" > "${WEZMUX_TTY:-/dev/tty}" 2>/dev/null || true ;;
esac
case "$cron_count" in
    ''|*[!0-9]*) ;;
    *) printf '\033]7777;session_crons;%s\007' "$cron_count" > "${WEZMUX_TTY:-/dev/tty}" 2>/dev/null || true ;;
esac
# Subagent count is managed exclusively by on-subagent-start/stop hooks.
# Claude Code's Stop hook input does not currently expose a reason field
# (no stop_button/interrupt distinction — see anthropics/claude-code#9516),
# so we can't safely force-reset here: a concurrent turn may legitimately
# have background subagents still running.  When Claude Code exits entirely,
# the sidebar's foreground-process check clears agent state via
# mux.remove_agent_status().  Orphaned /tmp/wezmux-subagents-<session_id>
# files from crashed sessions are harmless (unique session_id).
# Notification store
printf '\033]9;%s\007' "$msg" > "${WEZMUX_TTY:-/dev/tty}" 2>/dev/null || true
exit 0
