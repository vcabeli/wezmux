# Feature Landscape

**Domain:** Terminal workspace manager with per-workspace metadata sidebar and OSC notification system
**Researched:** 2026-03-24
**Confidence:** MEDIUM-HIGH (primary source is cmux which is the direct inspiration; tmux/Zellij/kitty/iTerm2 verified via official docs and HN discussion)

---

## Context

This research covers two intersecting product categories:

1. **Terminal workspace managers** — tmux, Zellij, GNU screen, cmux, WezTerm built-in workspaces
2. **Notification-capable terminals** — kitty (OSC 99), iTerm2 (OSC 9), cmux (OSC 9/99/777), Ghostty

Wezmux sits at the intersection: a terminal emulator (WezTerm fork) with workspace management (cmux-inspired sidebar) and notification routing (OSC sequences -> visual indicators).

The direct design reference is **cmux** (Ghostty-based macOS terminal with vertical tabs and notifications for AI coding agents). The feature map below is calibrated against cmux as the closest comparable product.

---

## Table Stakes

Features users expect from any terminal workspace manager. Missing = product feels broken or incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Session/workspace persistence across restarts | Every multiplexer (tmux-resurrect, iTerm2, Zellij) provides this; users assume work survives crashes | Med | WezTerm already has workspace model; needs scroll-back + layout save |
| Workspace switching by keyboard | tmux, Zellij, screen all require keyboard-first navigation | Low | Cmd+1..9 and workspace launcher already planned |
| Pane splitting (horizontal + vertical) | Foundational — tmux, Zellij, WezTerm all do this | Low | WezTerm already supports this |
| Named workspaces/sessions | tmux sessions, Zellij sessions, WezTerm workspaces all use names | Low | WezTerm Mux layer already has workspace labels |
| Visual indication of active workspace | Every multiplexer highlights current context | Low | Active workspace highlight in sidebar |
| Create new workspace | Standard action in all multiplexers | Low | Cmd+Shift+N already planned |
| Click-to-switch workspace | cmux does this; expected for GUI terminals | Low | Sidebar click handling planned |
| OSC 9 notification support | iTerm2 and Windows Terminal support OSC 9; widely used by tools like Claude Code | Low | Parsing in termwiz |
| Notification auto-clear on focus | cmux clears on workspace focus; expected lifecycle behavior | Low | mark_read on pane focus |
| Desktop notification passthrough | kitty, iTerm2, Ghostty all pass OSC sequences to the OS | Med | macOS UNUserNotificationCenter |
| Tab bar showing current workspace content | All multiplexers show tabs/windows | Low | WezTerm already has tab bar |
| Working directory display | cmux shows cwd; standard metadata | Low | Via OSC 7 or get_current_working_dir() |
| Keyboard shortcut to jump to unread | cmux has Cmd+Shift+U; users need quick navigation when managing many agents | Low | Planned as JumpToUnreadNotification |

---

## Differentiators

Features that set wezmux apart. Not universally expected, but highly valued by the target user (multi-agent developer workflows).

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Per-workspace git branch + dirty indicator in sidebar | No terminal emulator shows this without a shell prompt plugin; at-a-glance code context without switching panes | Med | Background polling via `git rev-parse` + `git status --porcelain` |
| Per-workspace PR status in sidebar | Unique to cmux; eliminates tab-switching to check PR state while agents run | High | gh CLI dependency, graceful degradation if absent, 30-60s poll interval |
| Per-workspace listening ports in sidebar | Unique to cmux; critical for multi-service agent setups | Med | lsof/proc polling every 5-10s |
| Blue notification ring around panes | cmux's signature visual; immediately obvious which pane needs attention without scanning all panes | Med | Rendered as colored border quads in WezTerm renderer |
| Unread notification badge on sidebar entries | Combines workspace-level view with unread state — no other multiplexer does this at the emulator level | Low | Badge rendering via ComputedElement |
| Latest notification text in sidebar | Cross-workspace text preview without opening notification panel | Low | Notification store lookup per workspace |
| OSC 9/99/777 all supported | cmux supports all three; other terminals pick one. Supporting all means existing agent tool integrations work without reconfiguration | Low | Three OSC handlers in termwiz |
| Notification suppression when workspace is focused | cmux suppresses desktop alerts when target workspace is active — reduces noise | Low | Focus-check before firing desktop notification |
| Custom notification command hook | cmux exposes CMUX_NOTIFICATION_TITLE/BODY env vars to a custom shell command — enables text-to-speech, logging, custom sounds | Med | Shell-out on notification event |
| Sidebar toggle (Cmd+B) | cmux has this; power users want maximum terminal real estate | Low | ToggleSidebar already planned |
| Background metadata polling (non-blocking) | Git/PR/port data updates without visible latency or render hiccup | High | Dedicated background thread, channel-based updates to render thread |
| Notification store with per-pane unread tracking | Enables future features (notification history, replay) and is architecturally clean | Med | NotificationStore in mux layer |

---

## Anti-Features

Features to explicitly NOT build in v1. Inclusion risks scope creep, maintenance burden, or user confusion.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| In-app browser | cmux has this; requires a full browser engine, security surface, isolated profiles. Not core to terminal workflow | Keep terminal-only; link to external browser via URL click |
| Analytics/dashboard panel | cmux has workspace analytics; adds complexity without clear value for individual developers | Surface port + PR info which is actionable; skip historical charts |
| Notification panel modal overlay | Deferred to v2 per PROJECT.md; Cmd+Shift+U covers the primary need | Sidebar latest-text + jump-to-unread is sufficient for v1 |
| Sidebar drag-to-reorder | cmux supports this; low-value polish item relative to implementation cost | Keyboard-only reorder in v2 |
| Sidebar context menu (rename, close) | Right-click context menus require modal management; defer polish | Keyboard-based rename/close in v2 |
| Sidebar resize by dragging | Dynamic resize requires continuous hit-testing + layout recalc; defer | Fixed 220px width in v1 with config option |
| Lua API format-sidebar-entry event | Adds scripting surface before the feature is stable; premature extensibility | Ship stable sidebar first, expose Lua hook in v2 |
| Linux/Windows support | Adds cross-platform renderer testing overhead; not the target user | macOS only for v1 using Cocoa+CGL |
| Smooth animations for sidebar toggle | CSS-style animation in a custom GPU renderer requires easing + frame timing; disproportionate effort | Instant show/hide in v1 |
| Session sharing / multiplayer | Zellij has this; requires networking and auth; not relevant for solo agent workflows | Not planned |
| Scriptable browser control (automation) | cmux's agent-browser API goes beyond terminal scope | OSC notifications + CLI are sufficient for agent integration |
| Pane zoom full-screen mode | HN commenters requested this for cmux; WezTerm has it built-in, but sidebar interaction during zoom is complex | WezTerm's native Cmd+Z already works; sidebar auto-hides if needed |

---

## Feature Dependencies

```
Workspace sidebar shell (layout + empty panel)
  -> Workspace list rendering (names, active indicator)
    -> Click-to-switch workspace
    -> New workspace button
    -> Working directory display
      -> Git branch polling (requires cwd)
        -> Git dirty indicator (same poll)
          -> PR status polling (requires cwd + gh CLI)
      -> Port polling (requires per-workspace pane cwd)
    -> Notification badge (requires Notification Store)
      -> Latest notification text (requires Notification Store)

Notification Store (mux layer)
  -> OSC 9 parsing  (independent, feeds store)
  -> OSC 99 parsing (independent, feeds store)
  -> OSC 777 parsing (independent, feeds store)
    -> Blue notification ring on panes (requires Store + renderer)
    -> Sidebar unread badge (requires Store + sidebar)
    -> Jump-to-unread (Cmd+Shift+U) (requires Store)
    -> Auto-mark-read on focus (requires Store + focus event)
    -> Desktop notification passthrough (requires Store + macOS API)
```

Key ordering constraint: **Sidebar shell must precede metadata** — you need the layout working before layering in data polling. **Notification Store must precede all visual indicators** — rings and badges are consumers of store state.

---

## MVP Recommendation

Prioritize in order:

1. **Sidebar shell** — layout carve-out, empty panel, toggle (Cmd+B), resize handling
2. **Workspace list** — names, active highlight, click-to-switch, new workspace button
3. **Working directory + git metadata** — cwd, branch, dirty flag in sidebar cards
4. **Notification store + OSC parsing** — push from terminal output, per-pane unread tracking
5. **Visual indicators** — blue ring + sidebar badge + jump-to-unread

This order matches the DESIGN.md phase plan and respects the dependency graph above.

Defer for v2:
- PR status: requires `gh` CLI dependency management; high complexity for marginal value until core UX is proven
- Port display: requires background `lsof` polling; medium complexity, defer until metadata polling is stable
- Notification panel modal: jump-to-unread satisfies the navigation need
- Custom notification command hook: nice-to-have personalization, not core
- Desktop notification passthrough: useful but not differentiating vs ring + badge

---

## Competitive Positioning

| Feature | tmux | Zellij | cmux | **Wezmux** |
|---------|------|--------|------|------------|
| Workspace/session model | Sessions | Sessions | Workspaces | Workspaces |
| Sidebar with metadata | No (status bar only) | No (tab bar only) | Yes (git, PR, ports) | Yes (git, branch, ports) |
| PR status in sidebar | No | No | Yes | Yes (v1 target) |
| Pane notification rings | No | No | Yes | Yes |
| OSC 9/99/777 | Plugin (tmux-notify) | No native | Yes, all 3 | Yes, all 3 |
| Session persistence | Plugin (resurrect) | Built-in (resurrection) | Yes (layout+cwd) | Inherits WezTerm |
| Plugin/extensibility | TPM ecosystem | WASM | CLI/socket API | Lua (WezTerm) |
| GPU rendering | No | No | Yes (libghostty) | Yes (WezTerm) |
| macOS native | No | No | Yes (Swift/AppKit) | Yes (Cocoa/CGL) |
| In-app browser | No | No | Yes | **No (anti-feature)** |

Wezmux's position: **cmux's workspace UX on top of WezTerm's superior terminal emulation**, without cmux's browser complexity. The WezTerm base also brings Lua scripting, cross-platform potential, and a mature community.

---

## Sources

- cmux README and docs: https://github.com/manaflow-ai/cmux (HIGH confidence — primary design reference)
- cmux notification docs: https://cmux.com/docs/notifications (HIGH confidence — official docs)
- HN discussion on cmux: https://news.ycombinator.com/item?id=47079718 (MEDIUM confidence — community feedback)
- kitty desktop notifications: https://sw.kovidgoyal.net/kitty/desktop-notifications/ (HIGH confidence — official docs)
- Zellij features: https://zellij.dev/features/ (HIGH confidence — official docs)
- tmux wiki: https://github.com/tmux/tmux/wiki/Getting-Started (HIGH confidence — official docs)
- WezTerm workspace recipes: https://wezterm.org/recipes/workspaces.html (HIGH confidence — official docs)
- OSC 9/99/777 comparison: https://github.com/gdamore/tcell/issues/499 (MEDIUM confidence — community discussion)
- tmux-notify plugin: https://github.com/rickstaa/tmux-notify (MEDIUM confidence — community plugin)
- tmux-resurrect: https://github.com/tmux-plugins/tmux-resurrect (HIGH confidence — widely adopted plugin)
