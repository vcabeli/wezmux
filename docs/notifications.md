# Notifications

Wezmux tracks notifications from terminal panes and surfaces them on the [sidebar](sidebar.md) as unread badges and preview text. This makes it easy to see which workspace needs attention without switching to it.

## How notifications are generated

Any program can send a notification by writing an **OSC 9** escape sequence to the terminal:

```bash
printf '\033]9;Build complete\007'
```

This is the same sequence used by iTerm2 and other terminals for system notifications. In Wezmux, it also feeds the notification store.

The Wezmux [hook scripts](agent-integration.md) for Claude Code and Codex emit OSC 9 alongside [OSC 7777](osc7777.md), so agent events automatically appear as notifications too.

## Notification lifecycle

1. **Created** -- an OSC 9 sequence arrives on a pane. A `Notification` record is stored with the pane ID, workspace name, body text, and timestamp. Marked as `unread`.
2. **Deduplicated** -- if the most recent notification for the same pane has identical text, the new one is silently dropped.
3. **Displayed** -- the sidebar shows an unread badge (count) on the workspace card and the latest notification body as preview text.
4. **Read** -- when the user switches to the pane (focuses it), all notifications for that pane are automatically marked as read. The unread badge disappears.
5. **Evicted** -- the store is capped at 1,000 notifications. The oldest are dropped when the cap is reached.

## Visual indicators

### Unread badge

A small circular badge appears in the top-right corner of the workspace card title when there are unread notifications:

- Shows the count: `1` through `9`, or `9+` for 9 or more
- Active workspace: white badge with subtle transparency
- Inactive workspace: accent-colored badge (blue by default)

### Preview text

The latest notification body is shown on the workspace card as muted preview text. When an agent is running, the [OSC 7777 message](osc7777.md) takes priority; the notification text is used as a fallback.

## API for scripts

Any program can participate in the notification system by emitting OSC 9:

```bash
# Simple notification
printf '\033]9;Deploy finished\007'

# From a build script
npm run build && printf '\033]9;Build succeeded\007' || printf '\033]9;Build failed!\007'
```

Notifications are per-pane: the escape sequence is attributed to whichever pane the program is running in.

## See also

- [OSC 7777](osc7777.md) -- structured agent status protocol (complements notifications)
- [Agent Integration](agent-integration.md) -- hook scripts that emit both OSC 9 and OSC 7777
- [Sidebar](sidebar.md) -- where notifications are displayed
