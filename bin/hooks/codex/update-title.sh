#!/bin/bash
# Emit a Codex conversation title to the Wezmux sidebar.
#
# Many Codex sessions never get a short threads.name. Use a bounded first
# prompt as a fallback, including the hook payload before the database is
# populated. With --wait, upgrade that fallback if a name arrives later.

command -v jq >/dev/null 2>&1 || exit 0
prompt=""

if [ "$1" = "--hook" ]; then
    input=$(cat 2>/dev/null)
    thread_id="${CODEX_THREAD_ID:-${CODEX_SESSION_ID:-}}"
    payload_thread_id=$(printf '%s' "$input" | jq -r '.session_id // .thread_id // empty' 2>/dev/null)
    [ -n "$payload_thread_id" ] && thread_id="$payload_thread_id"
    prompt=$(printf '%s' "$input" | jq -r '.prompt | strings' 2>/dev/null)
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

normalize_title() {
    # jq counts Unicode characters, so truncation cannot split a UTF-8 sequence.
    # Remove terminal controls and fold whitespace before bounding the heading.
    jq -Rrs '
        gsub("[\u0000-\u001f\u007f-\u009f]"; " ") |
        gsub("\\s+"; " ") | sub("^ +"; "") | sub(" +$"; "") |
        if length > 80 then .[:77] | sub(" +$"; "") + "..." else . end
    '
}

last_title=""
emit_title() {
    [ -n "$1" ] && [ "$1" != "$last_title" ] || return 0
    printf '\033]7777;title;%s\007' "$1" 2>/dev/null > "$tty_path" || return 1
    last_title="$1"
}

refresh_title() {
    fallback_title=""
    if command -v sqlite3 >/dev/null 2>&1; then
        for state_db in "$codex_state_dir"/state_*.sqlite; do
            [ -f "$state_db" ] || continue
            title=$(sqlite3 -readonly -noheader "$state_db" \
                "SELECT name FROM threads WHERE id = '$thread_id' LIMIT 1;" \
                2>/dev/null) || title=""
            title=$(printf '%s' "$title" | normalize_title)
            if [ -n "$title" ]; then
                emit_title "$title"
                return 0
            fi

            if [ -z "$fallback_title" ]; then
                # Query separately so older databases without name still work.
                # Prefer the saved first prompt to keep follow-ups from renaming
                # the conversation to something unhelpful such as "yes".
                fallback_title=$(sqlite3 -readonly -noheader "$state_db" \
                    "SELECT title FROM threads WHERE id = '$thread_id' LIMIT 1;" \
                    2>/dev/null) || fallback_title=""
                fallback_title=$(printf '%s' "$fallback_title" | normalize_title)
            fi
        done
    fi

    [ -n "$fallback_title" ] || fallback_title=$(printf '%s' "$prompt" | normalize_title)
    emit_title "$fallback_title"
    # A fallback must not stop --wait from looking for a later conversation name.
    return 1
}

if refresh_title || [ "$mode" != "--wait" ] || ! command -v sqlite3 >/dev/null 2>&1; then
    exit 0
fi

# Codex runs this hook with `async: true`, so polling does not hold up the
# conversation. The fallback is already visible; unchanged titles aren't resent.
attempt=0
while [ "$attempt" -lt 60 ]; do
    sleep 1
    refresh_title && exit 0
    attempt=$((attempt + 1))
done

exit 0
