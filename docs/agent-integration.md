# Agent Integration

Wezmux detects AI coding agents running in your terminal and shows their status on the [sidebar](sidebar.md). It works with Claude Code, Oh My Pi, Codex, Cursor, Aider, and OpenCode.

## How agent detection works

Wezmux scans the process tree of each pane to identify known agent executables. Detection is automatic -- no configuration needed. When an agent is found, the sidebar card shows:

- An **agent icon** next to the workspace title
- A **status indicator**: Working (with spinner animation), Idle, or Needs Input
- A **status message**: what the agent is currently doing

If the agent process exits, the status is cleared after a short grace period.

## Supported agents

| Agent | Detection | Structured status (OSC 7777) | Hooks provided |
|-------|-----------|------------------------------|----------------|
| **Claude Code** | `claude` in process name | Yes, via hooks | `bin/hooks/on-*.sh` |
| **Oh My Pi** | `omp` process name | Yes, via extension | `bin/hooks/omp/wezmux.js` |
| **Codex** | `codex` in process name | Yes, via hooks | `bin/hooks/codex/on-*.sh` |
| **Cursor** | `cursor` in process name | Fallback to OSC 9 | -- |
| **Aider** | `aider` in process name | Fallback to OSC 9 | -- |
| **OpenCode** | `opencode` in process name | Fallback to OSC 9 | -- |

Agents without hooks still get basic sidebar presence (icon + detection), but structured status (working/idle/needs_input) requires [OSC 7777](osc7777.md) integration.

## Claude Code integration

Wezmux ships a `claude` wrapper in its terminal-only `PATH`. Inside Wezmux, the
wrapper injects the bundled lifecycle hooks with Claude Code's `--settings`
option. Outside Wezmux it passes through to the real `claude` binary unchanged.
No setup is required.

If a caller supplies its own `--settings` option, the wrapper leaves it
unchanged instead of replacing it.

### What each hook does

| Hook | Event | Emits |
|------|-------|-------|
| `on-prompt-submit.sh` | User sends a prompt | `status;working` + notification |
| `on-notification.sh` | Claude sends a notification | `message;...` + notification. Promotes "needs attention" messages to `status;needs_input` |
| `on-needs-input.sh` | Claude asks a question (AskUserQuestion tool) | `status;needs_input` + notification |
| `on-stop.sh` | Claude finishes a turn | `status;idle` + `message;...` (preview of Claude's response) + notification |
| `on-subagent-start.sh` | Claude spawns a subagent | `subagents;N` (incremented count) |
| `on-subagent-stop.sh` | A subagent completes | `subagents;N` (decremented count) |

Each hook emits both OSC 7777 (for the agent status store) and OSC 9 (for the notification store), so the sidebar gets structured status and notification counts.

## Oh My Pi integration

Wezmux ships an `omp` wrapper beside the Claude Code wrapper. Inside Wezmux, it
loads `bin/hooks/omp/wezmux.js` with `omp --extension`; outside Wezmux it passes
through unchanged. No setup is required.

The extension reports prompt/agent lifecycle, tool activity, `ask` and tool
approval waits, final response previews, and completion notifications.

## Installing Codex hooks

Codex integration uses the scripts in `bin/hooks/codex/`. Install them after
installing Wezmux:

```bash
make install-codex-hooks
```

The installer merges Wezmux entries into `~/.codex/hooks.json`, preserves
unrelated hooks, enables `[features].hooks` in `~/.codex/config.toml`, and can be
run again safely after upgrades.

Codex generates its short conversation name asynchronously after a thread is
created. An asynchronous prompt hook watches for that title, and later
tool/stop hooks refresh it. They read the generated `threads.name` for the
hook's `session_id` from Codex's local state database and emit it as
`OSC 7777;title`; the full first prompt in `threads.title` and the terminal tab
title are not used for the card heading.

## Writing hooks for other agents

Any agent that supports lifecycle hooks can integrate with Wezmux by emitting [OSC 7777](osc7777.md) sequences. The minimum integration is:

```bash
# When the agent starts working
printf '\033]7777;status;working\007' > /dev/tty

# When the agent finishes
printf '\033]7777;status;idle\007' > /dev/tty

# When the agent needs user input
printf '\033]7777;status;needs_input\007' > /dev/tty
```

Add OSC 9 notifications alongside for unread badges:

```bash
printf '\033]9;Agent finished\007' > /dev/tty
```

Write to `/dev/tty` (not stdout) so the escape sequences reach the terminal even when the agent's stdout is piped or redirected.

## Status indicators

The sidebar shows agent status with distinct visual indicators:

| Status | Symbol | Color | When |
|--------|--------|-------|------|
| **Working** | Spinning braille animation | Orange | Agent is processing |
| **Idle** | Solid circle | Green | Agent is done |
| **Needs Input** | Triangle | Yellow / accent | Agent is waiting for user action |

## See also

- [OSC 7777](osc7777.md) -- the escape sequence protocol reference
- [Notifications](notifications.md) -- how OSC 9 notifications work
- [Sidebar](sidebar.md) -- where agent status is displayed
