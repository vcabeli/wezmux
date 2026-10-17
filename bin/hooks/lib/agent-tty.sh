# wezmux hook helper: resolve the terminal of the agent that ran this hook.
# Source it, then: tty=$(wezmux_agent_tty) || exit 0
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
            *) return 1 ;;
        esac
        pid=$ppid
        depth=$((depth + 1))
    done
    return 1
}
