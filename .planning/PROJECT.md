# Wezmux

## What This Is

A fork of WezTerm that adds cmux-inspired workspace management: a persistent sidebar showing per-workspace metadata (git branch, PR status, ports, notifications) and an OSC-based notification system with visual indicators (blue rings on panes, sidebar badges). Built as a personal tool for managing multiple agent workspaces in a single terminal window.

## Core Value

See at a glance which workspace needs attention — the sidebar and notification rings must make it obvious where work is happening and where input is needed.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Workspace sidebar panel on the left edge of the window (toggle with Cmd+B)
- [ ] Sidebar shows list of all workspaces with active workspace highlighted
- [ ] Click sidebar entry to switch workspace
- [ ] Sidebar displays working directory, git branch, dirty indicator per workspace
- [ ] Sidebar displays PR status (via gh CLI) per workspace
- [ ] Sidebar displays listening ports per workspace
- [ ] Background polling for git/PR/port metadata (non-blocking)
- [ ] Terminal content area shifts to accommodate sidebar width
- [ ] Resize handling when sidebar toggles or changes width
- [ ] Notification store for tracking per-pane notification state
- [ ] OSC 9/99/777 escape sequences push notifications to the store
- [ ] Blue ring border around panes with unread notifications
- [ ] Sidebar shows unread notification badge per workspace
- [ ] Sidebar shows latest notification text per workspace
- [ ] Auto-mark-read when user focuses a pane
- [ ] Cmd+Shift+U to jump to most recent unread pane
- [ ] Create new workspace from sidebar (Cmd+Shift+N)

### Out of Scope

- Browser/markdown panel (cmux feature, not needed) — complexity, not core
- Analytics dashboard (cmux feature) — not relevant for terminal
- `wezmux` CLI tool — defer to v2, OSC sequences sufficient for v1
- Notification panel modal overlay — defer to v2
- Sidebar drag-to-reorder — defer to v2 polish
- Sidebar context menu (rename, close) — defer to v2 polish
- Sidebar resize by dragging edge — defer to v2 polish
- Smooth animations — defer to v2 polish
- Lua API (`format-sidebar-entry` event) — defer to v2
- Linux/Windows support — macOS only for v1
- Sidebar `position = "right"` option — left-only for v1

## Context

- WezTerm is a Rust terminal emulator with a custom GPU-accelerated renderer (OpenGL/WebGPU). No native UI widgets — everything is drawn as textured quads.
- The `fancy_tab_bar` already uses a box-model layout engine (`ComputedElement` / `box_model.rs`) that can be reused for sidebar rendering.
- WezTerm's `Mux` layer already has workspace support: `active_workspace()`, `set_active_workspace()`, `iter_windows_in_workspace()`. Workspaces are string labels on mux `Window` objects.
- The `UIItem`/`UIItemType` system in `mouseevent.rs` handles hit-testing for mouse events and is extensible.
- OSC sequences are parsed in `termwiz/src/escape/osc.rs` — some notification-related OSC codes may already be partially handled.
- The fork does not yet exist — first step is to fork/clone the WezTerm repository.

## Constraints

- **Platform**: macOS only for v1 (Cocoa + CGL backend)
- **Renderer**: All UI must be drawn via WezTerm's custom renderer — no native widgets available
- **Dependencies**: `gh` CLI may not be installed — PR status must degrade gracefully
- **Performance**: Git/PR/port polling must never block the render thread
- **Memory**: Notification store capped at ~1000 entries to prevent unbounded growth

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Fork WezTerm rather than build from scratch | Leverages mature terminal emulation, GPU renderer, and existing workspace model | — Pending |
| Sidebar first, notifications second | Sidebar is the primary UX; notifications layer on top | — Pending |
| Reuse box_model.rs / ComputedElement for sidebar | Same engine powers fancy tab bar — proven, already in codebase | — Pending |
| macOS only for v1 | Personal tool, reduce cross-platform complexity | — Pending |
| Phases 1-5 for v1, defer CLI/polish to v2 | Focus on core sidebar + notification experience | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd:transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd:complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-03-24 after initialization*
