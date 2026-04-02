# Agent Integration

Wezmux detects AI coding agents running in your terminal and shows their status on the [sidebar](sidebar.md). It works with Claude Code, Codex, Cursor, Aider, and OpenCode.

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
| **Codex** | `codex` in process name | Yes, via hooks | `bin/hooks/codex/on-*.sh` |
| **Cursor** | `cursor` in process name | Fallback to OSC 9 | -- |
| **Aider** | `aider` in process name | Fallback to OSC 9 | -- |
| **OpenCode** | `opencode` in process name | Fallback to OSC 9 | -- |

Agents without hooks still get basic sidebar presence (icon + detection), but structured status (working/idle/needs_input) requires [OSC 7777](osc7777.md) integration.

## Installing Claude Code hooks

Wezmux ships hook scripts that connect Claude Code's lifecycle events to the sidebar via [OSC 7777](osc7777.md).

### Quick install

```bash
./bin/install-hooks.sh
```

This copies the hook scripts to `~/.claude/hooks/wezmux/` and prints the settings.json snippet to add.

### Manual setup

1. Copy the scripts from `bin/hooks/` to `~/.claude/hooks/wezmux/`:

    ```bash
    mkdir -p ~/.claude/hooks/wezmux
    cp bin/hooks/on-*.sh ~/.claude/hooks/wezmux/
    chmod +x ~/.claude/hooks/wezmux/*.sh
    ```

2. Add the hooks to `~/.claude/settings.json`:

    ```json
    {
      "hooks": {
        "Notification": [
          { "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux/on-notification.sh", "timeout": 5 }] }
        ],
        "Stop": [
          { "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux/on-stop.sh", "timeout": 5 }] }
        ],
        "UserPromptSubmit": [
          { "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux/on-prompt-submit.sh", "timeout": 5 }] }
        ],
        "PreToolUse": [
          { "matcher": "AskUserQuestion", "hooks": [{ "type": "command", "command": "~/.claude/hooks/wezmux/on-needs-input.sh", "timeout": 5 }] }
        ]
      }
    }
    ```

### What each hook does

| Hook | Event | Emits |
|------|-------|-------|
| `on-prompt-submit.sh` | User sends a prompt | `status;working` + notification |
| `on-notification.sh` | Claude sends a notification | `message;...` + notification. Promotes "needs attention" messages to `status;needs_input` |
| `on-needs-input.sh` | Claude asks a question (AskUserQuestion tool) | `status;needs_input` + notification |
| `on-stop.sh` | Claude finishes a turn | `status;idle` + `message;...` (preview of Claude's response) + notification |

Each hook emits both OSC 7777 (for the agent status store) and OSC 9 (for the notification store), so the sidebar gets structured status and notification counts.

## Installing Codex hooks

Codex hooks work the same way but are in `bin/hooks/codex/`:

```bash
mkdir -p ~/.claude/hooks/wezmux-codex
cp bin/hooks/codex/on-*.sh ~/.claude/hooks/wezmux-codex/
chmod +x ~/.claude/hooks/wezmux-codex/*.sh
```

Then wire them into your Codex configuration following the same pattern as the Claude Code hooks above.

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
