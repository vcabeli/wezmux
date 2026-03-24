# Requirements: Wezmux

**Defined:** 2026-03-24
**Core Value:** See at a glance which workspace needs attention — sidebar and notification rings make it obvious where work is happening and where input is needed.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Fork Setup

- [ ] **FORK-01**: WezTerm source is forked and builds successfully on macOS
- [ ] **FORK-02**: Upstream remote is configured for periodic rebasing
- [ ] **FORK-03**: Project compiles and runs as a standard WezTerm instance before any modifications

### Sidebar Layout

- [ ] **SIDE-01**: Sidebar panel renders on the left edge of the window with a dark background
- [ ] **SIDE-02**: Sidebar has a fixed width (~220px) configurable via wezterm.lua
- [ ] **SIDE-03**: Terminal content area shifts right to accommodate sidebar width
- [ ] **SIDE-04**: PTY column count is recalculated correctly when sidebar is visible (cell-aligned)
- [ ] **SIDE-05**: Sidebar can be toggled on/off with Cmd+B
- [ ] **SIDE-06**: All visible panes receive SIGWINCH-equivalent resize when sidebar toggles
- [ ] **SIDE-07**: Sidebar renders correctly on macOS Retina (HiDPI) displays

### Workspace List

- [ ] **WKSP-01**: Sidebar displays a card for each workspace with the workspace name
- [ ] **WKSP-02**: Active workspace card has a highlighted/selected background
- [ ] **WKSP-03**: Clicking a workspace card switches to that workspace
- [ ] **WKSP-04**: Hovering a workspace card shows a highlight effect
- [ ] **WKSP-05**: Cmd+Shift+N creates a new named workspace
- [ ] **WKSP-06**: Each workspace card shows the working directory of the active pane
- [ ] **WKSP-07**: Each workspace card shows pane and tab count

### Workspace Metadata

- [ ] **META-01**: Each workspace card displays the current git branch name
- [ ] **META-02**: Each workspace card displays a dirty indicator when the git working tree has changes
- [ ] **META-03**: Git branch and dirty status are polled in a background thread (every 3-5s)
- [ ] **META-04**: Each workspace card displays PR number and state (open/merged/closed) via gh CLI
- [ ] **META-05**: PR status is polled in a background thread (every 30-60s)
- [ ] **META-06**: PR status gracefully degrades when gh CLI is not installed (shows nothing, no error)
- [ ] **META-07**: Each workspace card displays listening TCP ports for that workspace
- [ ] **META-08**: Port detection is polled in a background thread (every 5-10s)
- [ ] **META-09**: All background polling never blocks the render thread

### Notification Store

- [ ] **NOTF-01**: A NotificationStore in the mux layer tracks per-pane notification state
- [ ] **NOTF-02**: OSC 9 escape sequences push notifications to the store
- [ ] **NOTF-03**: OSC 99 (kitty) escape sequences push notifications to the store
- [ ] **NOTF-04**: OSC 777 (rxvt) escape sequences push notifications to the store
- [ ] **NOTF-05**: Notification store is capped at ~1000 entries (oldest evicted)
- [ ] **NOTF-06**: Notifications are associated with pane ID and workspace name

### Visual Indicators

- [ ] **VIND-01**: Panes with unread notifications display a bright cyan/blue border ring (2-3px)
- [ ] **VIND-02**: The notification ring disappears when the user focuses/clicks that pane
- [ ] **VIND-03**: Multiple panes can have notification rings simultaneously
- [ ] **VIND-04**: Sidebar workspace cards display an unread notification badge (count or dot)
- [ ] **VIND-05**: Sidebar workspace cards display the latest notification text as a muted line
- [ ] **VIND-06**: Focusing a pane marks all its notifications as read (auto-read behavior)

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### CLI Tool

- **CLI-01**: `wezmux notify` CLI sends notifications to running Wezmux instance
- **CLI-02**: CLI communicates via WezTerm's client-server codec (new SendNotification PDU)
- **CLI-03**: CLI supports targeting by pane-id or workspace name

### Notification Panel

- **PANEL-01**: Cmd+I opens a modal overlay showing all notifications
- **PANEL-02**: Notifications grouped by workspace with timestamps
- **PANEL-03**: Click notification to jump to source pane

### Polish

- **POLISH-01**: Jump-to-unread via Cmd+Shift+U (focus most recent unread pane)
- **POLISH-02**: Sidebar drag-to-reorder workspaces
- **POLISH-03**: Sidebar context menu (rename, close workspace)
- **POLISH-04**: Sidebar resize by dragging edge
- **POLISH-05**: Smooth animations for sidebar show/hide
- **POLISH-06**: Lua API: `format-sidebar-entry` event for customization

### Platform

- **PLAT-01**: Linux support (X11 + Wayland backends)
- **PLAT-02**: Windows support (Win32 backend)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| In-app browser | Complexity, security surface, not core to terminal workflow |
| Analytics/dashboard panel | Not relevant for individual developer use |
| Session sharing / multiplayer | Not relevant for solo agent workflows |
| Custom notification command hook | Defer until notification system is proven |
| Desktop notification passthrough (macOS) | OSC 9 crashes unsigned dev builds; in-process indicators are primary path |
| Sidebar `position = "right"` option | Left-only simplifies v1 |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FORK-01 | Phase 1 | Pending |
| FORK-02 | Phase 1 | Pending |
| FORK-03 | Phase 1 | Pending |
| SIDE-01 | Phase 2 | Pending |
| SIDE-02 | Phase 2 | Pending |
| SIDE-03 | Phase 2 | Pending |
| SIDE-04 | Phase 2 | Pending |
| SIDE-05 | Phase 2 | Pending |
| SIDE-06 | Phase 2 | Pending |
| SIDE-07 | Phase 2 | Pending |
| WKSP-01 | Phase 3 | Pending |
| WKSP-02 | Phase 3 | Pending |
| WKSP-03 | Phase 3 | Pending |
| WKSP-04 | Phase 3 | Pending |
| WKSP-05 | Phase 3 | Pending |
| WKSP-06 | Phase 3 | Pending |
| WKSP-07 | Phase 3 | Pending |
| META-01 | Phase 4 | Pending |
| META-02 | Phase 4 | Pending |
| META-03 | Phase 4 | Pending |
| META-04 | Phase 4 | Pending |
| META-05 | Phase 4 | Pending |
| META-06 | Phase 4 | Pending |
| META-07 | Phase 4 | Pending |
| META-08 | Phase 4 | Pending |
| META-09 | Phase 4 | Pending |
| NOTF-01 | Phase 5 | Pending |
| NOTF-02 | Phase 5 | Pending |
| NOTF-03 | Phase 5 | Pending |
| NOTF-04 | Phase 5 | Pending |
| NOTF-05 | Phase 5 | Pending |
| NOTF-06 | Phase 5 | Pending |
| VIND-01 | Phase 6 | Pending |
| VIND-02 | Phase 6 | Pending |
| VIND-03 | Phase 6 | Pending |
| VIND-04 | Phase 6 | Pending |
| VIND-05 | Phase 6 | Pending |
| VIND-06 | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 38 total
- Mapped to phases: 38
- Unmapped: 0

---
*Requirements defined: 2026-03-24*
*Last updated: 2026-03-24 after roadmap creation*
