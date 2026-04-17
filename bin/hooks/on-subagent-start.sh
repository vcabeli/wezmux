#!/bin/bash
# wezmux hook: Claude Code SubagentStart event
# Tracks running subagent count via temp file, emits OSC 7777 subagents event.
#
# Install: copy to ~/.claude/hooks/wezmux/ and wire in ~/.claude/settings.json

input=$(cat 2>/dev/null)

session_id=""
if command -v jq >/dev/null 2>&1; then
    session_id=$(echo "$input" | jq -r '.session_id // empty' 2>/dev/null)
fi

[ -z "$session_id" ] && exit 0

count_file="/tmp/wezmux-subagents-${session_id}"
lock_dir="${count_file}.lock"

# Use mkdir as a portable atomic lock (works on macOS and Linux).
# Install trap first so a SIGTERM mid-acquire can't orphan the lock.
# Break stale locks after ~2s (hook timeout is 5s, so a live holder would
# have finished by then) to recover from SIGKILL'd previous invocations.
trap 'rmdir "$lock_dir" 2>/dev/null' EXIT
_attempts=0
while ! mkdir "$lock_dir" 2>/dev/null; do
    _attempts=$(( _attempts + 1 ))
    if [ "$_attempts" -gt 200 ]; then
        rmdir "$lock_dir" 2>/dev/null
        mkdir "$lock_dir" 2>/dev/null || exit 0
        break
    fi
    sleep 0.01
done

count=0
[ -f "$count_file" ] && count=$(cat "$count_file" 2>/dev/null)
count=$(( count + 1 ))
echo "$count" > "$count_file"
printf '\033]7777;subagents;%s\007' "$count" > /dev/tty 2>/dev/null || true

exit 0
