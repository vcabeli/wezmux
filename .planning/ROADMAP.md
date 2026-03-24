# Roadmap: Wezmux

## Overview

Wezmux is built in six phases, each delivering a coherent capability. Fork setup establishes the upstream-tracked foundation. The sidebar shell follows, then workspace list rendering and navigation. Background metadata polling (git, PR, ports) layers on top of the workspace cards. The notification store and OSC parsing provide the data substrate for visual indicators. Finally, blue pane rings and sidebar badges complete the "see at a glance which workspace needs attention" experience.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Fork Setup** - Fork WezTerm, configure upstream remote, verify clean build on macOS
- [ ] **Phase 2: Sidebar Shell** - Empty sidebar panel renders, toggles with Cmd+B, PTY columns recalculate correctly
- [ ] **Phase 3: Workspace List** - Sidebar shows workspace cards, click-to-switch, new workspace creation
- [ ] **Phase 4: Workspace Metadata** - Background polling for git branch, dirty state, PR status, and listening ports
- [ ] **Phase 5: Notification Store** - OSC 9/99/777 sequences populate a per-pane notification store in the Mux layer
- [ ] **Phase 6: Visual Indicators** - Blue pane rings, sidebar unread badges, latest notification text, jump-to-unread

## Phase Details

### Phase 1: Fork Setup
**Goal**: The project exists as a buildable macOS fork of WezTerm with upstream tracking in place
**Depends on**: Nothing (first phase)
**Requirements**: FORK-01, FORK-02, FORK-03
**Success Criteria** (what must be TRUE):
  1. Running `cargo build` on the fork succeeds and produces a working WezTerm binary on macOS
  2. The upstream WezTerm remote is configured and `git fetch upstream` retrieves the latest commits without errors
  3. The fork launches and behaves identically to stock WezTerm — no regressions, no crashes
**Plans**: 1 plan
Plans:
- [ ] 01-01-PLAN.md — Fork WezTerm, configure upstream remote, build and verify on macOS

### Phase 2: Sidebar Shell
**Goal**: Users can toggle an empty sidebar panel that correctly shifts terminal content and recalculates PTY column count
**Depends on**: Phase 1
**Requirements**: SIDE-01, SIDE-02, SIDE-03, SIDE-04, SIDE-05, SIDE-06, SIDE-07
**Success Criteria** (what must be TRUE):
  1. Pressing Cmd+B shows and hides a dark sidebar panel on the left edge of the window
  2. Running `tput cols` inside a pane returns the correct (reduced) column count when the sidebar is visible, and the original column count when hidden
  3. Running `vim` and toggling the sidebar causes vim to reflow its layout correctly without manual resize
  4. The sidebar renders crisply without blurriness on a macOS Retina display
**Plans**: 1 plan
Plans:
- [ ] 01-01-PLAN.md — Fork WezTerm, configure upstream remote, build and verify on macOS
**UI hint**: yes

### Phase 3: Workspace List
**Goal**: Users can see all workspaces in the sidebar, navigate between them by clicking, and create new ones
**Depends on**: Phase 2
**Requirements**: WKSP-01, WKSP-02, WKSP-03, WKSP-04, WKSP-05, WKSP-06, WKSP-07
**Success Criteria** (what must be TRUE):
  1. Each workspace appears as a card in the sidebar showing its name, active pane working directory, and pane/tab count
  2. The active workspace card has a visually distinct highlighted background; inactive cards do not
  3. Clicking any workspace card immediately switches to that workspace
  4. Pressing Cmd+Shift+N prompts for a workspace name and creates a new workspace that immediately appears in the sidebar
  5. Hovering over a workspace card shows a hover highlight effect
**Plans**: 1 plan
Plans:
- [ ] 01-01-PLAN.md — Fork WezTerm, configure upstream remote, build and verify on macOS
**UI hint**: yes

### Phase 4: Workspace Metadata
**Goal**: Each workspace card in the sidebar shows live git branch, dirty indicator, PR status, and listening ports — all polled without blocking the render thread
**Depends on**: Phase 3
**Requirements**: META-01, META-02, META-03, META-04, META-05, META-06, META-07, META-08, META-09
**Success Criteria** (what must be TRUE):
  1. Each workspace card shows the current git branch name and a dirty indicator when the working tree has uncommitted changes, updating every 3-5 seconds
  2. Each workspace card shows the PR number and state (open/merged/closed) for the current branch, updating every 30-60 seconds
  3. When `gh` CLI is not installed, the PR status area is blank — no error message, no crash
  4. Each workspace card shows the TCP ports currently listening in that workspace, updating every 5-10 seconds
  5. The terminal remains responsive (no frame drops, no input lag) while all polling is active
**Plans**: 1 plan
Plans:
- [ ] 01-01-PLAN.md — Fork WezTerm, configure upstream remote, build and verify on macOS
**UI hint**: yes

### Phase 5: Notification Store
**Goal**: OSC 9, OSC 99, and OSC 777 escape sequences from any pane populate a capped per-pane notification store in the Mux layer
**Depends on**: Phase 2
**Requirements**: NOTF-01, NOTF-02, NOTF-03, NOTF-04, NOTF-05, NOTF-06
**Success Criteria** (what must be TRUE):
  1. Running `printf '\e]9;test notification\a'` in a pane records a notification associated with that pane ID and its workspace
  2. OSC 777 (`printf '\e]777;notify;title;body\a'`) and OSC 99 sequences are also routed to the notification store
  3. Sending more than 1000 notifications to a single pane evicts the oldest entries — the store never exceeds 1000 entries total
  4. Notifications retrieved from the store correctly carry the pane ID and workspace name they originated from
**Plans**: 1 plan
Plans:
- [ ] 01-01-PLAN.md — Fork WezTerm, configure upstream remote, build and verify on macOS

### Phase 6: Visual Indicators
**Goal**: Users can see at a glance which panes have unread notifications via blue rings and sidebar badges, and can jump directly to the most urgent one
**Depends on**: Phase 5
**Requirements**: VIND-01, VIND-02, VIND-03, VIND-04, VIND-05, VIND-06
**Success Criteria** (what must be TRUE):
  1. A pane that has received an OSC notification displays a bright cyan/blue 2-3px border ring; panes without notifications do not
  2. Multiple panes can simultaneously display notification rings; clicking or focusing any pane immediately clears its ring
  3. Each workspace card in the sidebar shows an unread notification badge (count or dot) when any pane in that workspace has unread notifications
  4. Each workspace card shows a muted one-line preview of the latest notification text from that workspace
**Plans**: 1 plan
Plans:
- [ ] 01-01-PLAN.md — Fork WezTerm, configure upstream remote, build and verify on macOS
**UI hint**: yes

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Fork Setup | 0/TBD | Not started | - |
| 2. Sidebar Shell | 0/TBD | Not started | - |
| 3. Workspace List | 0/TBD | Not started | - |
| 4. Workspace Metadata | 0/TBD | Not started | - |
| 5. Notification Store | 0/TBD | Not started | - |
| 6. Visual Indicators | 0/TBD | Not started | - |
