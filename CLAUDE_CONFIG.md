# Claude Code Configuration for Wezmux

Wezmux shows Claude Code status and notifications in the sidebar via hooks that emit OSC 9 escape sequences. This requires installing hook scripts and adding entries to `~/.claude/settings.json`.

## Quick Setup

```bash
# Install hook scripts
./bin/install-hooks.sh
```

Then add the hooks to `~/.claude/settings.json` (see below).

## What the hooks do

| Hook Event | Script | What it emits |
|---|---|---|
| `Notification` | `on-notification.sh` | Forwards Claude's notification body as OSC 9 |
| `Stop` | `on-stop.sh` | "Claude finished" / "Claude stopped by user" / "Claude interrupted" |
| `UserPromptSubmit` | `on-prompt-submit.sh` | "Claude is working..." |
| `PreToolUse` (AskUserQuestion) | `on-needs-input.sh` | "Claude is waiting for your input" |

Each script writes `\033]9;<message>\007` to `/dev/tty`, which WezTerm's OSC parser picks up and routes to the notification store. The sidebar reads from the store to show preview text, unread badges, and blue pane rings.

## Hook scripts

Scripts live in two places:
- **Source**: `bin/hooks/` in this repo
- **Installed**: `~/.claude/hooks/wezmux/` (where settings.json references them)

## ~/.claude/settings.json

Add these entries to the `"hooks"` object. Merge with any existing hooks you have (e.g. GSD hooks):

```json
{
  "hooks": {
    "Notification": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "~/.claude/hooks/wezmux/on-notification.sh",
            "timeout": 5
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "~/.claude/hooks/wezmux/on-stop.sh",
            "timeout": 5
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "~/.claude/hooks/wezmux/on-prompt-submit.sh",
            "timeout": 5
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "AskUserQuestion",
        "hooks": [
          {
            "type": "command",
            "command": "~/.claude/hooks/wezmux/on-needs-input.sh",
            "timeout": 5
          }
        ]
      }
    ]
  }
}
```

## ~/.claude.json

Optional but recommended — set the notification channel to iterm2 so Claude Code's native notifications also use OSC 9:

```json
{
  "preferredNotifChannel": "iterm2"
}
```

Set via: `claude config set --global preferredNotifChannel iterm2`

## How it works

```
Claude Code fires hook event
  -> shell script runs as child process
  -> script writes OSC 9 to /dev/tty (the pane's PTY)
  -> WezTerm's OSC parser: OperatingSystemCommand::SystemNotification
  -> AlertHandler -> MuxNotification::Alert
  -> apply_notification_to_store() -> NotificationStore
  -> Sidebar reads latest notification per workspace
  -> Preview text + unread badge + blue pane ring
```

## Dependencies

- `jq` — optional but recommended. The notification and stop hooks use it to parse JSON from Claude Code's hook input. Without jq, they fall back to generic messages.

## Codex

Codex (OpenAI) emits OSC 9 natively — no hooks needed. It works out of the box in wezmux.

## Verifying it works

1. Build and run wezmux: `cargo build -p wezterm && ./target/debug/wezterm`
2. Test OSC 9 manually: `printf '\e]9;Hello world\e\\'` — should show blue ring + sidebar badge
3. Start Claude Code in a pane — sidebar should show "Claude is working..." when you submit a prompt, and "Claude finished" or the actual response when done
