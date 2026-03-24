# Architecture Patterns

**Domain:** GPU-accelerated terminal emulator fork (WezTerm + sidebar/notifications)
**Researched:** 2026-03-24
**Confidence:** HIGH — based on WezTerm source analysis, DESIGN.md, and DeepWiki documentation

---

## System Overview

Wezmux is a fork of WezTerm, not a greenfield project. All new UI components must integrate
with WezTerm's existing rendering pipeline, type system, and event model. No native UI widgets
are available — everything is drawn as GPU-batched textured quads via OpenGL/WebGPU.

The codebase is a Cargo workspace of ~19 crates organized in three layers:

```
wezterm-gui        GUI + rendering (TermWindow, RenderState, GlyphCache, box_model)
    |
    v
mux                Session model (Mux singleton, Window, Tab, Pane trait)
    |
    v
wezterm-term / termwiz    Terminal emulation (VTE parser, Screen, escape sequences)
```

New code for wezmux lives primarily in `wezterm-gui` and `mux`, with a small touch to
`termwiz` for OSC parsing.

---

## Recommended Architecture

### High-Level Layout

```
+------------------------------------------------------------------------+
| Native macOS window (Cocoa + CGL)                                      |
+------------------------------------------------------------------------+
| TermWindow (wezterm-gui/src/termwindow/mod.rs)                         |
|  +---------+   +--------------------------------------------------+   |
|  | Sidebar |   | Tab Bar (fancy / retro)                          |   |
|  | State   |   +--------------------------------------------------+   |
|  |         |   | Pane 1        | Pane 2 [blue ring]               |   |
|  | [ws1]   |   |               |                                  |   |
|  | [ws2*]  |   |               |                                  |   |
|  | [ws3]   |   +---------------+----------------------------------+   |
|  | [+ new] |   | Pane 3                                           |   |
|  +---------+   +--------------------------------------------------+   |
+------------------------------------------------------------------------+
```

### Component Boundaries

| Component | Location | Responsibility | Communicates With |
|-----------|----------|---------------|-------------------|
| `TermWindow` | `wezterm-gui/src/termwindow/mod.rs` | GUI window owner, top-level coordinator | All components below |
| `SidebarState` | `wezterm-gui/src/termwindow/sidebar.rs` (new) | Sidebar visibility, width, cached workspace entries, poller handle | `TermWindow`, `Mux`, `NotificationStore` |
| `render/sidebar.rs` | `wezterm-gui/src/termwindow/render/sidebar.rs` (new) | Build `ComputedElement` tree and paint sidebar | `SidebarState`, `box_model.rs`, GPU quad layers |
| `box_model.rs` | `wezterm-gui/src/termwindow/box_model.rs` (existing) | CSS-like layout engine for GPU quads | `render/sidebar.rs`, `render/fancy_tab_bar.rs` |
| `render/paint.rs` | `wezterm-gui/src/termwindow/render/paint.rs` (modify) | Top-level paint orchestration, offset panes by sidebar width | `SidebarState`, all render sub-modules |
| `resize.rs` | `wezterm-gui/src/termwindow/resize.rs` (modify) | Compute available terminal dimensions; subtract sidebar width | `SidebarState`, `RenderMetrics` |
| `mouseevent.rs` | `wezterm-gui/src/termwindow/mouseevent.rs` (modify) | Hit-test UIItems, dispatch mouse events | `SidebarState`, `UIItemType` enum |
| `NotificationStore` | `mux/src/notification.rs` (new) | Track per-pane notification state, unread counts | `Mux`, `TermWindow` |
| `Mux` | `mux/src/lib.rs` (modify) | Global session registry; also owns `NotificationStore` | `TermWindow`, `wezterm-term`, `codec` |
| OSC handler | `termwiz/src/escape/osc.rs` or `term/` (modify) | Parse OSC 9/777, emit `MuxNotification::PaneNotification` | `Mux`, `NotificationStore` |
| Background poller | spawned thread inside `SidebarState` | Poll git branch, git dirty, PR status, listening ports | `SidebarState` via `Sender<Vec<WorkspaceEntry>>` |
| `config/keyassignment.rs` | `config/src/keyassignment.rs` (modify) | New `KeyAssignment` variants: `ToggleSidebar`, `JumpToUnreadNotification` | `InputMap`, `TermWindow` |

---

## Data Flow

### Rendering Pipeline (Frame Draw)

```
OS repaint event
    |
    v
TermWindow::paint()
    -> paint_impl()
        -> paint_pass()
            1. Reset ui_items vector
            2. Render background
            3. For each positioned pane:
               - paint_pane()   (terminal content)
               - paint_pane_border() or paint_notification_ring()  [NEW]
            4. paint_tab_bar()
            5. paint_sidebar()   [NEW — after tab bar, above everything else]
               -> builds ComputedElement tree from SidebarState.entries
               -> renders quads via TripleLayerQuadAllocator
               -> populates ui_items with SidebarEntry / SidebarNewButton regions
            6. GPU draw call (RenderState)
```

Pane offset insertion point (in `paint_pass` / `get_panes_to_render`):
```
available_left_offset += if sidebar.visible { sidebar.width } else { 0.0 }
```

### Resize Event Flow

```
Window resize or sidebar toggle
    |
    v
TermWindow::apply_dimensions()
    -> avail_width = pixel_width
                   - padding_left - padding_right
                   - border_left - border_right
                   - sidebar_width   [NEW]
    -> rows = avail_height / cell_height
    -> cols = avail_width  / cell_width
    -> Mux::notify(MuxNotification::WindowInvalidated)
    -> All panes resize their internal screen buffers
    -> schedule_next_frame()
```

### Mouse Event Flow

```
OS mouse event (x, y)
    |
    v
TermWindow::mouse_event()
    -> resolve_ui_item(x, y)   // reverse-iterate ui_items, find hit
        |
        +-- UIItemType::SidebarEntry { workspace_index }
        |       -> mux.set_active_workspace(entry.workspace_name)
        |       -> apply_dimensions() to re-render filtered workspace
        |
        +-- UIItemType::SidebarNewButton
        |       -> open workspace name prompt (modal overlay)
        |
        +-- UIItemType::Tab / Split / Scrollbar / ...   (existing)
        |
        +-- No hit -> forward to pane content
    |
    v
Hover tracking: update sidebar.hovered, trigger repaint
```

### Notification Data Flow

```
Terminal program writes OSC 9/777 to PTY stdout
    |
    v
PtyReader thread reads bytes
    -> Terminal::advance_bytes()
    -> Performer::osc_dispatch()   (termwiz or term/)
    -> emit MuxNotification::PaneNotification { pane_id, title, body }
    |
    v
Mux::notify() dispatches to subscribers
    -> NotificationStore::push(notification)
    -> sets unread_panes.insert(pane_id)
    |
    v
TermWindowNotif::MuxNotification delivered to TermWindow
    -> SidebarState.entries[ws].unread_count updated
    -> SidebarState.entries[ws].latest_notification updated
    -> schedule repaint
    |
    v
Next frame: paint_notification_ring() draws blue border on unread panes
            paint_sidebar() draws badge on workspace card
```

### Background Polling Flow

```
Dedicated thread (spawned once at SidebarState::new())
    loop every 3s:
        for each workspace:
            cwd = active_pane.get_current_working_dir()
            git_branch = run("git rev-parse --abbrev-ref HEAD") in cwd
            git_dirty  = run("git status --porcelain") in cwd
        every 5s:
            ports = run("lsof -iTCP -sTCP:LISTEN")
        every 30s:
            pr_info = run("gh pr view --json number,state") in cwd
        tx.send(Vec<WorkspaceEntry>)
    |
    v
TermWindow receives on rx channel (checked at start of paint or via waker)
    -> SidebarState.entries = new_entries
    -> rebuild ComputedElement tree
    -> schedule repaint
```

---

## Key Architectural Constraints

### No Native Widgets
All UI is rendered as GPU quads. The sidebar, notification rings, and badges are pixel-painted.
There is no system for layout reflow (no DOM, no Cocoa views). Sizing must be deterministic
from data available at paint time.

### Box Model Engine Capabilities and Limits
`ComputedElement` / `box_model.rs` supports: block/inline flow, padding, margin, borders with
corner radius, per-side border colors, static/animated colors, vertical alignment, float-right.

It does NOT support: flex-grow, justify-content, align-items, scrollable containers, or
two-pass measurement. The sidebar height is fixed (all entries visible; no scrolling in v1).
This is sufficient because the fancy tab bar already uses this engine successfully.

### Sidebar Width as Left Padding
The cleanest integration point is treating sidebar width as additional left padding fed into
`apply_dimensions()`. This flows naturally into the existing `avail_width` calculation
without requiring changes to pane position tracking downstream.

The alternative — offsetting each pane individually in `get_panes_to_render()` — is more
surgical but risks missing edge cases (e.g., split coordinate calculations, resize increments).

### UIItem System is Extensible
`UIItemType` is a plain Rust enum. Adding `SidebarEntry { workspace_index: usize }` and
`SidebarNewButton` is a non-breaking change. Hit-testing uses `iter().rev().find()` so
sidebar items (painted last, on top) will naturally win over pane content for overlapping
regions.

### OSC 9 Already Parsed — Hook, Don't Rewrite
WezTerm already dispatches OSC 9 and OSC 777 to produce system toast notifications
(confirmed present since version 20240127). The `OperatingSystemCommand` enum in termwiz
already has variants for these. The work is to intercept at the dispatch site and also push
to `NotificationStore`, rather than replacing the existing handler.

OSC 99 (kitty) is not currently listed in WezTerm's documented escape sequences. It may
exist in the enum or require a new variant — needs verification when reading the source.

### MuxNotification Channel
`TermWindow` subscribes to mux events via `TermWindowNotif::MuxNotification`. Adding a
`PaneNotification` variant follows the existing pattern used for `Alert`, `WindowInvalidated`,
and `AssignClipboard`. The `Mux::notify()` broadcast fan-out is already present.

### Background Thread Must Not Block Render Thread
Git and `gh` subprocesses can take 100-500ms. They must run exclusively on a dedicated
background thread with the results sent over a channel. The render thread only reads
the latest snapshot. Stale data between polls is acceptable.

---

## Patterns to Follow

### Pattern 1: ComputedElement Tree Construction (copy from fancy_tab_bar.rs)

The fancy tab bar in `render/fancy_tab_bar.rs` constructs an `Element` tree, passes it to
`self.compute_element(&render_context, &element)`, and receives a `ComputedElement` for
painting. Sidebar rendering follows the same pattern:

```rust
// In render/sidebar.rs
pub fn build_sidebar_element(state: &SidebarState, metrics: &RenderMetrics) -> Element {
    // build Element tree from state.entries
    // return root Element
}

// In TermWindow during paint:
let element = build_sidebar_element(&self.sidebar, &self.render_metrics);
let computed = self.compute_element(&ctx, &element)?;
self.render_element(&computed, &mut layers, Some(&mut self.ui_items))?;
```

### Pattern 2: Modal Overlay (copy from palette.rs / charselect.rs)

The notification panel (future phase) reuses the `modal.rs` pattern — an overlay `Pane`
that captures input and renders over the terminal content. The command palette is the
reference implementation.

### Pattern 3: Config Extension (copy from any config option)

Sidebar config lives in `config/src/lib.rs` as a new `SidebarConfig` struct with
`#[derive(Debug, Clone, Deserialize)]` and `impl Default`. The Lua API uses the existing
`FromLua` / `ToLua` conversion machinery already present for all config options.

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: Storing Sidebar Width as a Separate Coordinate System
**What:** Tracking pane pixel offsets separately from padding
**Why bad:** The existing resize pipeline already uses `avail_width` as the single source
of truth for pane grid dimensions. Adding a parallel offset breaks resize/column calculations.
**Instead:** Inject sidebar width into `apply_dimensions()` as additional left margin, just
like `padding_left`.

### Anti-Pattern 2: Polling on the Render Thread
**What:** Calling `git` or `gh` during `paint_pass()`
**Why bad:** These subprocesses take 100-500ms. At 60fps the render loop budget is 16ms.
A single blocked poll freezes the UI.
**Instead:** Background thread with `Sender/Receiver<Vec<WorkspaceEntry>>`. Render thread
reads the latest snapshot atomically; never waits.

### Anti-Pattern 3: Embedding NotificationStore in TermWindow
**What:** Putting notification state in the GUI layer
**Why bad:** Multiple GUI windows exist; if each window has its own store, notifications sent
to a pane visible in window A are not reflected in window B.
**Instead:** `NotificationStore` lives in `Mux` (the shared session layer), consistent with
how pane state is owned by `Mux` and observed by `TermWindow`.

### Anti-Pattern 4: Unbounded Notification Store
**What:** Appending to `NotificationStore.notifications` without eviction
**Why bad:** Long-running processes (e.g., agent workspaces running for days) can generate
thousands of notifications. Uncapped memory growth causes OOM.
**Instead:** Cap at 1000 entries, evict oldest on overflow. Already identified in DESIGN.md.

### Anti-Pattern 5: Reconstructing ComputedElement Every Frame
**What:** Rebuilding the full `ComputedElement` tree on every `paint_pass()` call
**Why bad:** The box model computation is CPU-bound and allocates; at 60fps this wastes cycles.
**Instead:** Cache the `ComputedElement` in `SidebarState.computed`. Invalidate only when
`entries` changes (new data from poll channel or notification event).

---

## Component Build Order

Each phase adds components in dependency order:

```
Phase 1: Sidebar Shell
  New:    SidebarState struct (sidebar.rs)
  New:    ToggleSidebar KeyAssignment (config/keyassignment.rs)
  Modify: TermWindow::mod.rs — add SidebarState field, init, Cmd+B handler
  Modify: render/paint.rs — compute sidebar_width offset, call paint_sidebar()
  Modify: resize.rs — subtract sidebar_width from avail_width
  Modify: mouseevent.rs — add UIItemType::Sidebar, UIItemType::SidebarEntry
  Result: Empty sidebar panel appears/disappears, terminal content shifts
  No dependency on later phases.

Phase 2: Workspace List (static)
  New:    render/sidebar.rs — ComputedElement tree for workspace cards
  Modify: sidebar.rs — populate entries from Mux workspace list
  Modify: mouseevent.rs — handle SidebarEntry click -> set_active_workspace
  Depends on: Phase 1 (SidebarState, paint call, UIItem types)

Phase 3: Workspace Metadata
  New:    Background polling thread in sidebar.rs
  Modify: WorkspaceEntry struct — git_branch, git_dirty, pr_info, ports fields
  Modify: render/sidebar.rs — render metadata rows in workspace cards
  Depends on: Phase 2 (workspace card rendering exists)

Phase 4: NotificationStore + OSC Handling
  New:    mux/src/notification.rs — NotificationStore
  Modify: mux/src/lib.rs — embed NotificationStore, add MuxNotification::PaneNotification
  Modify: termwiz or term/ OSC handler — hook OSC 9/777 dispatch
  Modify: render/sidebar.rs — show unread badge and latest notification text
  Depends on: Phase 2 (sidebar rendering for badge display)

Phase 5: Visual Indicators
  Modify: render/pane.rs — paint_notification_ring() for unread panes
  Modify: focus handling — mark_read on pane focus
  New:    JumpToUnreadNotification KeyAssignment
  Depends on: Phase 4 (NotificationStore with unread state)

Phase 6: CLI + Notification Panel
  New:    wezmux-cli/ crate
  Modify: codec/src/lib.rs — SendNotification PDU
  New:    Notification panel modal overlay
  Depends on: Phase 4 (NotificationStore), Phase 5 (visual indicators)
```

---

## Files to Create

| File | Phase | Description |
|------|-------|-------------|
| `wezterm-gui/src/termwindow/sidebar.rs` | 1 | SidebarState, WorkspaceEntry, polling thread |
| `wezterm-gui/src/termwindow/render/sidebar.rs` | 2 | ComputedElement tree, paint sidebar |
| `mux/src/notification.rs` | 4 | NotificationStore, Notification, NotificationSource |
| `wezmux-cli/src/main.rs` | 6 | CLI tool entry point |
| `wezmux-cli/Cargo.toml` | 6 | CLI crate manifest |

## Files to Modify

| File | Phase | Change |
|------|-------|--------|
| `wezterm-gui/src/termwindow/mod.rs` | 1 | Add `sidebar: SidebarState` field |
| `wezterm-gui/src/termwindow/render/paint.rs` | 1 | Sidebar offset, `paint_sidebar()` call, `paint_notification_ring()` call |
| `wezterm-gui/src/termwindow/render/mod.rs` | 2 | `mod sidebar;` |
| `wezterm-gui/src/termwindow/resize.rs` | 1 | Subtract `sidebar_width` from `avail_width` in `apply_dimensions()` |
| `wezterm-gui/src/termwindow/mouseevent.rs` | 1 | Add `UIItemType::SidebarEntry`, `UIItemType::SidebarNewButton`; handle clicks/hover |
| `config/src/lib.rs` | 1/3 | `SidebarConfig` struct with poll intervals, width, visibility |
| `config/src/keyassignment.rs` | 1/5 | `ToggleSidebar`, `JumpToUnreadNotification` variants |
| `mux/src/lib.rs` | 4 | Embed `NotificationStore`, add `MuxNotification::PaneNotification` |
| `termwiz/src/escape/osc.rs` or `term/` | 4 | Hook OSC 9/777 dispatch to push `MuxNotification::PaneNotification` |
| `codec/src/lib.rs` | 6 | `SendNotification` PDU variant |
| `Cargo.toml` (workspace) | 6 | Add `wezmux-cli` member |

---

## Scalability Considerations

| Concern | With 5 workspaces | With 20+ workspaces |
|---------|-------------------|---------------------|
| Sidebar height | Fits in window | Needs scroll or overflow; defer to v2 |
| Poll thread load | Negligible | Linear in workspace count; acceptable |
| `gh` PR calls | ~1 per workspace per 30s | Rate limiting possible; add exponential backoff |
| ComputedElement rebuild | Fast (~1ms) | Still fast; entries are data-small |
| NotificationStore memory | Trivial | Cap at 1000 entries; evict oldest |

---

## Sources

- [WezTerm GUI Application Structure — DeepWiki](https://deepwiki.com/wezterm/wezterm/3.1-gui-frontend) (HIGH confidence — generated from source)
- [WezTerm User Interface — DeepWiki](https://deepwiki.com/wezterm/wezterm/3-user-interface) (HIGH confidence)
- [WezTerm crate overview — DeepWiki](https://deepwiki.com/wezterm/wezterm) (HIGH confidence)
- [WezTerm box_model.rs source analysis](https://github.com/wezterm/wezterm/blob/main/wezterm-gui/src/termwindow/box_model.rs) (HIGH confidence)
- [WezTerm mouseevent.rs source analysis](https://github.com/wezterm/wezterm/blob/main/wezterm-gui/src/termwindow/mouseevent.rs) (HIGH confidence)
- [WezTerm escape sequences — official docs](https://wezterm.org/escape-sequences.html) (HIGH confidence — OSC 9, OSC 777 confirmed)
- [WezTerm notification_handling config — official docs](https://wezterm.org/config/lua/config/notification_handling.html) (HIGH confidence)
- [WezTerm workspace API — official docs](https://wezterm.org/config/lua/wezterm.mux/set_active_workspace.html) (HIGH confidence)
- DESIGN.md in this repository (authoritative — written by project owner with deep WezTerm knowledge)
