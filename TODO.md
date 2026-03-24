# Wezmux — Gap Analysis vs cmux

Side-by-side comparison from screenshots taken 2026-03-25.
Research based on cmux source code analysis (see `.planning/cmux-research-report.md`).

---

## What we HAVE (implemented and working)

- [x] Sidebar with box_model/ComputedElement rendering
- [x] Blue active card background, lighter gray inactive cards with visible bg
- [x] Workspace name displayed in cards (bold title font)
- [x] Git branch + path shown on cards (smaller body font)
- [x] PR status line ("PR #19 merged") shown on cards
- [x] Agent detection (Claude Code, Codex, Cursor, etc.) via process tree scanning
- [x] Agent status inference from notification text
- [x] Unread badge with count
- [x] Drag-to-resize sidebar
- [x] Git/PR/port metadata polling (background)
- [x] Left accent bar on active card
- [x] Notification ring on panes (blue border, flush with pane edges)
- [x] Auto-mark-read on focus
- [x] Mark-read on sidebar card click (works with single pane)
- [x] Cmd+B toggle sidebar
- [x] Cmd+Shift+U jump to unread
- [x] Option+1..9 workspace switching
- [x] Sidebar scrolling (mouse wheel)
- [x] OSC 9 notification pipeline (code path complete in Rust)
- [x] Claude Code hooks → OSC 9 → notification store → sidebar
- [x] Click to switch workspace
- [x] Terminal output preview on sidebar cards (cached, 200ms refresh)
- [x] Smart preview: prefers terminal buffer over generic "Claude finished" when idle
- [x] Preview filters: skips box-drawing, status bars, prompts, separators
- [x] Close button (×) on workspace cards (hover)
- [x] Top toolbar with + / bell / split buttons
- [x] Settings gear icon at sidebar bottom
- [x] Better path display (full ~/path, end-truncated with ...)
- [x] Increased card padding for better readability
- [x] Font hierarchy: bold 12pt title, regular 11pt body/meta
- [x] Color hierarchy: lighter sidebar bg, darker card bg, muted text for meta
- [x] Sidebar entry caching (only rebuilds on structural/notification changes)
- [x] Terminal preview caching (200ms interval, not per-frame)

---

## How cmux does it (key insights from source analysis)

cmux does NOT read the terminal buffer for output preview. It does NOT use process scanning for agent detection. Instead:

1. **Claude wrapper script** — a bash script at `Resources/bin/claude` intercepts the `claude` command. When inside cmux (detected via `CMUX_SURFACE_ID` env var), it injects **6 lifecycle hooks** via Claude Code's `--settings` JSON flag: `SessionStart`, `Stop`, `SessionEnd`, `Notification`, `UserPromptSubmit`, `PreToolUse`. Each hook calls `cmux claude-hook <subcommand>` over a Unix domain socket.

2. **Output preview comes from the Notification hook** — Claude Code fires notifications that include the output text. cmux displays the latest notification body on the sidebar card. That's how "Hi! How can I help?" appears.

3. **Agent status comes from hooks** — `UserPromptSubmit` → "Running", `PreToolUse` → clears "Needs input", `Stop` → completion notification. This is real-time, not inferred.

4. **Shell integration via Unix domain sockets** — shell precmd/preexec hooks report cwd, git branch (reads `.git/HEAD` directly, no subprocess), PR status (`gh pr list` every 45s), ports (`lsof` batched scanning). All sent via socket IPC, not OSC.

5. **The toolbar is minimal** — just shows the focused command text. The "bell, split buttons" visible in screenshots are part of the sidebar/window chrome, not an NSToolbar.

---

## What's MISSING (ordered by impact, informed by cmux architecture)

### P0 — Claude Code notification integration ✅ SOLVED

**Status**: Working via Claude Code hooks → OSC 9 → notification store → sidebar.

**Solution**: Claude Code hooks (Notification, Stop, UserPromptSubmit, PreToolUse) emit OSC 9 to `/dev/tty`. The existing Rust pipeline picks up OSC 9 → `Alert::ToastNotification` → `NotificationStore` → sidebar preview + unread badge + blue pane ring.

**What's configured** (in `~/.claude/settings.json`):
- `Notification` hook → `on-notification.sh` — forwards notification body as OSC 9
- `Stop` hook → `on-stop.sh` — emits "Claude finished (reason)" as OSC 9
- `UserPromptSubmit` hook → `on-prompt-submit.sh` — emits "Claude is working..." as OSC 9
- `PreToolUse` (matcher: `AskUserQuestion`) → `on-needs-input.sh` — emits "Claude is waiting for your input" as OSC 9

**Hook scripts**: `bin/hooks/` (repo) and `~/.claude/hooks/wezmux/` (installed). Install with `bin/install-hooks.sh`.

### P1 — Terminal output preview ✅ DONE

- [x] Read last line from active pane's terminal buffer via `pane.get_lines()`
- [x] Prefer terminal buffer over generic notifications when agent is idle
- [x] Fall back to notification text when buffer scan finds nothing (Codex compat)
- [x] Filters: box-drawing chars, status bars, prompts, separators, short fragments
- [x] Cached at 200ms interval (not per-frame) for performance with 10+ workspaces

### P2 — Sidebar card layout polish ✅ DONE

- [x] Increased card padding (12px horizontal, 10px vertical)
- [x] Better path display — full `~/path` with end-truncation
- [x] Close button (×) on hover
- [x] Font weight hierarchy — bold 12pt title, regular 11pt body/meta
- [x] Preview text wraps to 4 lines (up from 2)
- [x] Lighter sidebar bg with darker card bg (cards visually distinct)
- [x] Square card corners (clean, no rendering artifacts)

### P3 — Top toolbar / chrome ✅ DONE

- [x] Toolbar row with + / bell / split-horizontal / split-vertical buttons
- [x] Replaced "workspaces" header with toolbar
- [x] Moved + new to toolbar
- [ ] Bell click opens notification panel (Cmd+I — not yet implemented)

### P4 — Bottom bar / settings ✅ DONE

- [x] Settings gear icon at sidebar bottom
- [ ] Settings gear click action (currently no-op, needs config panel or wezterm.lua opening)

### P5 — Notification panel & polish

- [ ] **Notification panel (Cmd+I)** — modal overlay or sidebar mode showing notification history
- [ ] **Notification suppression for active agent sessions** — suppress OSC desktop notifications when agent is active

### P6 — Sidebar UX

- [ ] **Right-click context menu** — rename, close, move workspace
- [ ] **Workspace reordering** — drag to reorder in sidebar
- [ ] **Smooth animations** — sidebar show/hide, card transitions

### P7 — Configuration & extensibility

- [ ] **Sidebar config exposed to Lua** — width, position, visibility, poll intervals
- [ ] **`format-sidebar-entry` Lua event** — customize card rendering
- [ ] **`sidebar-entry-clicked` Lua event** — custom click actions

---

## Performance

Sidebar rendering is optimized for 10+ workspaces:
- **Terminal preview**: cached per-workspace, refreshed every 200ms (not per-frame)
- **Sidebar entries**: cached between frames, invalidated on structural changes or notification count changes
- **Notification store**: O(1) HashMap lookups for unread counts
- **Metadata polling**: background tokio tasks, 200ms coalesce delay, 60s PR refresh

---

## Remaining work

1. **Notification panel (Cmd+I)** — modal overlay showing notification history
2. **Settings gear action** — open wezterm.lua or show config
3. **Bell icon action** — open notification panel
4. **Right-click context menu** on cards (rename, close)
5. **Smooth animations** — sidebar show/hide transitions
6. **Lua config exposure** — sidebar width, visibility, poll intervals
