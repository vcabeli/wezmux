#!/bin/bash
# Emit Codex's generated conversation title to the Wezmux sidebar.
#
# Codex stores the full first prompt in threads.title. The short title shown
# in Codex is threads.name, which is generated asynchronously after a new
# thread starts. With --wait, keep looking long enough for that first name to
# appear; later hook invocations use the default one-shot lookup.

if [ "$1" = "--hook" ]; then
    input=$(cat 2>/dev/null)
    thread_id="${CODEX_THREAD_ID:-${CODEX_SESSION_ID:-}}"
    if command -v jq >/dev/null 2>&1; then
        payload_thread_id=$(printf '%s' "$input" | jq -r '.session_id // .thread_id // empty' 2>/dev/null)
        [ -n "$payload_thread_id" ] && thread_id="$payload_thread_id"
    fi
    tty_path="${WEZMUX_TTY:-/dev/tty}"
    mode="${2:---wait}"
else
    thread_id="$1"
    tty_path="${2:-${WEZMUX_TTY:-/dev/tty}}"
    mode="${3:---once}"
fi
codex_state_dir="${CODEX_HOME:-$HOME/.codex}"

if ! printf '%s' "$thread_id" | grep -Eq '^[A-Za-z0-9_-]{1,128}$'; then
    exit 0
fi

emit_title() {
    command -v sqlite3 >/dev/null 2>&1 || return 1

    title=""
    for state_db in "$codex_state_dir"/state_*.sqlite; do
        [ -f "$state_db" ] || continue
        title=$(sqlite3 -readonly -noheader "$state_db" \
            "SELECT COALESCE(NULLIF(name, ''), '') FROM threads WHERE id = '$thread_id' LIMIT 1;" \
            2>/dev/null) || title=""
        [ -n "$title" ] && break
    done

    [ -n "$title" ] || return 1

    # Remove terminal controls, then collapse the title to the single line
    # used by the card. The OSC parser rejoins semicolons in event data.
    title=$(printf '%s' "$title" | tr '\n' ' ' | sed 's/  */ /g' | tr -d '\007\033')
    [ -n "$title" ] || return 1
    printf '\033]7777;title;%s\007' "$title" > "$tty_path" 2>/dev/null || true
    return 0
}

if emit_title || [ "$mode" != "--wait" ]; then
    exit 0
fi

# New conversation names are generated out of band. Codex runs this hook with
# `async: true`, so polling does not hold up the conversation.
attempt=0
while [ "$attempt" -lt 60 ]; do
    sleep 1
    emit_title && exit 0
    attempt=$((attempt + 1))
done

exit 0
