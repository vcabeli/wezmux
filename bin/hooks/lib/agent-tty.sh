# wezmux hook helper: resolve the terminal of the agent that ran this hook.
# Source it, then: tty=$(wezmux_agent_tty "$payload") || exit 0
#
# Hooks cannot simply write to /dev/tty: Claude Code 2.1.143+ starts them
# without a controlling terminal.  Nor can they trust an inherited variable
# naming the pane's tty: any long-lived process started from a Wezmux pane
# keeps that pane's environment forever.  The Codex app-server daemon runs
# every Codex session's hooks with the environment of whichever pane first
# spawned it, and a tmux server hands its starting pane's environment to every
# session — so a pane-tty variable puts one agent's status and preview on
# another workspace's card.
#
# Instead, walk up from the hook through the shells that wrap it to the first
# other process, and use that process's terminal.  When the agent runs in a
# pane, that is the agent itself (claude, codex).  When it has no terminal — a
# daemon, or `codex exec` / `claude -p` run by another agent — report nothing
# rather than write into some other pane.
#
# The exception is the Codex app-server daemon, which runs the hooks of every
# Codex TUI attached to it: it has no terminal, so the hook payload has to say
# which TUI the thread belongs to (see wezmux_codex_daemon_tty).
#
# WEZMUX_HOOK_TTY names an explicit destination and skips the walk (tests).
wezmux_agent_tty() {
    if [ -n "${WEZMUX_HOOK_TTY:-}" ]; then
        printf '%s' "$WEZMUX_HOOK_TTY"
        return 0
    fi

    local pid=$$ ppid tty comm depth=0
    while [ "$pid" -gt 1 ] && [ "$depth" -lt 8 ]; do
        read -r ppid tty comm <<<"$(ps -o ppid=,tty=,comm= -p "$pid" 2>/dev/null)"
        [ -n "$ppid" ] || return 1
        case "$tty" in
            '' | '??' | '-') ;;
            *)
                printf '/dev/%s' "$tty"
                return 0
                ;;
        esac
        case "${comm##*/}" in
            sh | bash | zsh | dash | -sh | -bash | -zsh) ;;
            codex)
                [ -n "${1:-}" ] &&
                    ps -o command= -p "$pid" 2>/dev/null | grep -q ' app-server' &&
                    wezmux_codex_daemon_tty "$1"
                return
                ;;
            *) return 1 ;;
        esac
        pid=$ppid
        depth=$((depth + 1))
    done
    return 1
}

# Resolve a daemon-hosted Codex hook to its TUI's terminal from the payload.
# Only TUI threads have a pane, and the TUI is identified by the thread's
# directory, so this answers only when exactly one Codex TUI runs there.
wezmux_codex_daemon_tty() {
    command -v jq >/dev/null 2>&1 && command -v sqlite3 >/dev/null 2>&1 || return 1
    local thread cwd db originator="" pids line pid="" t ttys=""
    thread=$(printf '%s' "$1" | jq -r '.session_id // .thread_id // empty' 2>/dev/null)
    cwd=$(printf '%s' "$1" | jq -r '.cwd // empty' 2>/dev/null)
    printf '%s' "$thread" | grep -Eq '^[A-Za-z0-9_-]{1,128}$' || return 1
    cwd=$(cd "$cwd" 2>/dev/null && pwd -P) || return 1

    for db in "${CODEX_HOME:-$HOME/.codex}"/state_*.sqlite; do
        [ -f "$db" ] || continue
        originator=$(sqlite3 -readonly -noheader "$db" \
            "SELECT originator FROM threads WHERE id = '$thread' LIMIT 1;" 2>/dev/null)
        [ -n "$originator" ] && break
    done
    [ "$originator" = codex-tui ] || return 1

    pids=$(ps -axo pid=,tty=,comm= 2>/dev/null |
        awk '$2 != "??" && $2 != "-" && /(^|\/)codex$/ { printf "%s%s", sep, $1; sep = "," }')
    [ -n "$pids" ] || return 1
    while read -r line; do
        case "$line" in
            p*) pid=${line#p} ;;
            n*)
                [ "${line#n}" = "$cwd" ] || continue
                t=$(ps -o tty= -p "$pid" 2>/dev/null | tr -d ' ')
                case "$t" in '' | '??' | '-') continue ;; esac
                case " $ttys " in *" $t "*) ;; *) ttys="$ttys $t" ;; esac
                ;;
        esac
    done < <(lsof -a -d cwd -p "$pids" -Fpn 2>/dev/null)

    set -- $ttys
    [ $# -eq 1 ] || return 1
    printf '/dev/%s' "$1"
}
