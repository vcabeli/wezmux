# Wezmux: WezTerm + cmux Sidebar & Notifications

## Goal

Fork WezTerm and extend it with cmux's two killer features:

1. **Workspace sidebar** — vertical tab bar showing per-workspace metadata (git branch, PR status, working directory, listening ports, latest notification)
2. **Notification system** — OSC-based + CLI-based notifications with visual indicators (blue ring on panes, lit-up sidebar tabs, unread badge, jump-to-unread)

The browser, markdown panel, and analytics from cmux are explicitly out of scope.

---

## Visual Reference (cmux screenshots)

These screenshots from cmux define the target UX:

### Hero — sidebar + terminal workspace
![main](https://raw.githubusercontent.com/manaflow-ai/cmux/main/docs/assets/main-first-image.png)

### Notification rings on panes
![rings](https://raw.githubusercontent.com/manaflow-ai/cmux/main/docs/assets/notification-rings.png)

### Sidebar notification badge / panel
![badge](https://raw.githubusercontent.com/manaflow-ai/cmux/main/docs/assets/sidebar-notification-badge.png)

### Vertical tabs with splits, git info, ports
![tabs](https://raw.githubusercontent.com/manaflow-ai/cmux/main/docs/assets/vertical-horizontal-tabs-and-splits.png)

### Full app with multiple splits (target screenshot)
![screenshot](https://raw.githubusercontent.com/manaflow-ai/cmux/refs/heads/main/docs/assets/screenshot.png)

---

## Precise Visual Spec (from cmux screenshots)

### Sidebar

- **Position:** left edge of window, dark background (slightly lighter than terminal bg)
- **Width:** ~200-220px, fixed (not proportional)
- **Entries are stacked vertically**, each is a compact card:
  - Workspace/tab name in bold white text
  - Metadata lines below in muted/gray text (smaller or same size)
  - Entries have subtle separators between them
- **Active workspace** has a highlighted/selected background (slightly lighter)
- **Notification badge:** small colored dot or count indicator on the workspace entry — appears when an agent finishes or sends a notification
- **Latest notification text** shown as a muted line under the workspace name
- The sidebar has a header area at the top and a bottom status/info area

### Pane Borders (Notification Ring)

- When a pane has an unread notification (agent waiting for input), it gets a **bright cyan/blue border** (2-3px) around the entire pane area
- This is distinct from the normal split divider — it's a glowing highlight ring
- The ring disappears when the user focuses/clicks that pane (marking it as read)
- Multiple panes can have rings simultaneously (you can see which agents need attention at a glance)

### Overall Layout

```
+----------------------------------------------------------+
| Title bar (native macOS)                                 |
+--------+-------------------------------------------------+
|        |  Tab bar (horizontal, shows tabs within         |
|        |  current workspace)                             |
| SIDE-  +------------------------+------------------------+
| BAR    | Pane 1 (terminal)      | Pane 2 (terminal)      |
|        | [optional blue ring]   | [optional blue ring]   |
| [ws1]  |                        |                        |
| [ws2*] |                        |                        |
| [ws3]  +------------------------+------------------------+
|        | Pane 3 (terminal)      | Pane 4 (terminal)      |
|        |                        |                        |
+--------+------------------------+------------------------+
| Status bar (bottom): workspace info, branch, ports       |
+----------------------------------------------------------+
```

### Color Palette (derived from screenshots, Monokai-like theme)

- Sidebar background: `#1e1e1e` (very dark gray)
- Sidebar entry active: `#2d2d2d` (slightly lighter)
- Sidebar text: `#cccccc` (light gray)
- Sidebar muted text: `#888888`
- Notification badge: `#e74c3c` (red circle)
- Pane notification ring: `#66d9ef` (bright cyan/blue)
- Active pane border: normal split color (subtle)
- Terminal background: `#272822` (Monokai standard)

---

## WezTerm Architecture Overview

WezTerm is a Rust application with a custom GPU-accelerated renderer. There are no native UI widgets — everything is drawn as textured quads via OpenGL/WebGPU. Understanding the architecture is critical because every UI element must be drawn manually.

### Crate Map

```
wezterm/              CLI entry point binary
wezterm-gui/          GUI application — this is where all rendering lives
  src/
    termwindow/
      mod.rs          TermWindow — the central struct owning all state
      render/
        paint.rs      Top-level paint orchestration (paint_pass, paint_impl)
        pane.rs       Pane content rendering
        tab_bar.rs    Retro tab bar rendering
        fancy_tab_bar.rs  Fancy (box-model) tab bar rendering  <-- REUSE THIS
        split.rs      Split divider rendering
        borders.rs    Window borders
      box_model.rs    Flexbox-like layout engine  <-- REUSE THIS
      mouseevent.rs   Mouse hit-testing (UIItem/UIItemType system)
      resize.rs       Terminal resize calculations
      modal.rs        Modal overlay system
    tabbar.rs         Tab bar data model (TabBarState, TabEntry)
mux/                  Multiplexer model layer (no rendering)
  src/
    lib.rs            Mux singleton — global pane/tab/window registry
    pane.rs           Pane trait
    tab.rs            Tab — binary tree of panes (splits)
    window.rs         Window — contains tabs + workspace label (string)
window/               Platform-native window abstraction
  src/os/
    macos/            Cocoa + CGL
    windows/          Win32
    x11/              XCB
    wayland/          Smithay
config/               Configuration + Lua scripting
termwiz/              Terminal emulation primitives, escape sequence parsing
term/                 Terminal state machine (screen buffer, scrollback)
```

### Key Types

| Type | Crate | Role |
|------|-------|------|
| `Mux` | `mux/` | Global singleton. Owns all `Window`s, `Tab`s, `Pane`s. Has `active_workspace()`, `set_active_workspace()` |
| `Window` | `mux/window.rs` | Contains `Vec<Tab>` + `workspace: String`. NOT a GUI window — a mux-level container |
| `Tab` | `mux/tab.rs` | Binary tree of `Pane`s via splits |
| `Pane` | `mux/pane.rs` | Trait — `LocalPane` (PTY-backed), `ClientPane` (remote mux), overlays |
| `TermWindow` | `wezterm-gui/termwindow/mod.rs` | The GUI window. Owns render state, tab bar, pane positions, overlays, modals, scrollbar |
| `ComputedElement` | `wezterm-gui/termwindow/box_model.rs` | Box-model layout node — used by fancy tab bar, reusable for sidebar |
| `UIItem` / `UIItemType` | `wezterm-gui/termwindow/mouseevent.rs` | Hit-test regions for mouse events. Extensible enum |

### Rendering Pipeline

```
TermWindow::paint()
  -> paint_impl()
    -> paint_pass(Pass::Background)
    -> paint_pass(Pass::Foreground)

paint_pass():
  1. Calculate padding (padding_left_top)
  2. Paint tab bar (paint_tab_bar)      <-- uses fancy_tab_bar ComputedElement tree
  3. For each positioned pane:
     - paint_pane()                      <-- terminal content
     - paint_pane_border()               <-- split dividers
  4. Paint scrollbar
  5. Paint modal overlay (if any)
```

### Workspace Model

Workspaces in WezTerm are string labels on mux `Window` objects. The `Mux` tracks an `active_workspace` string. Only windows matching the active workspace are shown. Switching workspace = changing the active workspace string and re-rendering.

Relevant mux API:
- `mux.active_workspace()` -> `String`
- `mux.set_active_workspace(name)`
- `mux.iter_windows_in_workspace(name)` -> list of `WindowId`
- Each `Window` has `.get_workspace()` and `.set_workspace(name)`

---

## Feature 1: Workspace Sidebar

### What It Shows

A vertical panel on the left side of the window. Each entry represents a **workspace** and displays:

| Field | Source | Update frequency |
|-------|--------|-----------------|
| Workspace name | `mux::Window::get_workspace()` | On workspace create/rename |
| Working directory | Active pane's `get_current_working_dir()` | On pane focus / OSC 7 |
| Git branch | Shell out to `git rev-parse --abbrev-ref HEAD` in cwd | Poll every 3-5s |
| Git dirty indicator | Shell out to `git status --porcelain` | Poll every 3-5s |
| PR status | Shell out to `gh pr view --json number,state` | Poll every 30-60s |
| Listening ports | Parse `lsof -iTCP -sTCP:LISTEN` or `/proc/net/tcp` | Poll every 5-10s |
| Latest notification | From notification store (see Feature 2) | On notification event |
| Unread indicator | From notification store | On notification event |
| Active indicator | `mux.active_workspace() == this_workspace` | On workspace switch |

### Layout Changes

Current layout:
```
+------------------------------------------+
| Tab Bar (fancy or retro)                 |
+------------------------------------------+
| Pane 1        | Pane 2                   |
|               | (split)                  |
|               |                          |
+------------------------------------------+
```

New layout:
```
+------------------------------------------+
| Tab Bar (fancy or retro)                 |
+--------+---------------------------------+
|        | Pane 1        | Pane 2          |
| SIDE-  |               | (split)         |
| BAR    |               |                 |
|        |               |                 |
| [ws1]  |               |                 |
| [ws2]  |               |                 |
| [ws3]  |               |                 |
|        +---------------------------------+
| [+new] |                                 |
+--------+---------------------------------+
```

### Implementation Plan

#### 1. Sidebar State (`wezterm-gui/src/termwindow/sidebar.rs` — new file)

```rust
pub struct SidebarState {
    /// Whether the sidebar is visible
    pub visible: bool,
    /// Width in pixels (configurable, default ~200px)
    pub width: f32,
    /// Cached workspace entries, refreshed periodically
    pub entries: Vec<WorkspaceEntry>,
    /// Currently hovered entry index (for highlight)
    pub hovered: Option<usize>,
    /// Computed box-model element tree (cached, rebuilt on data change)
    pub computed: Option<ComputedElement>,
    /// Background poller handle for git/PR/port data
    pub poller: Option<std::thread::JoinHandle<()>>,
}

pub struct WorkspaceEntry {
    pub workspace_name: String,
    pub cwd: String,
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub pr_info: Option<PrInfo>,
    pub listening_ports: Vec<u16>,
    pub latest_notification: Option<String>,
    pub unread_count: u32,
    pub is_active: bool,
    pub pane_count: usize,
    pub tab_count: usize,
}

pub struct PrInfo {
    pub number: u32,
    pub state: PrState, // Open, Merged, Closed
}
```

#### 2. Sidebar Rendering (`wezterm-gui/src/termwindow/render/sidebar.rs` — new file)

Build a `ComputedElement` tree using the existing box model engine:

```
Sidebar (vertical flex container, fixed width)
├── Header ("WORKSPACES" label)
├── WorkspaceCard (for each workspace)
│   ├── Name row:  [active indicator] [workspace name]
│   ├── Dir row:   folder icon + directory basename
│   ├── Branch row: git icon + branch name [dirty marker]
│   ├── PR row:    PR icon + #number + state (if any)
│   ├── Ports row: port icon + :3000, :8080 (if any)
│   └── Notification row: latest notification text (if any)
└── Footer ([+ New Workspace] button)
```

Each `WorkspaceCard` has:
- A border (rounded if the box model supports it, otherwise single-pixel)
- Active workspace gets accent-colored border + slightly lighter background
- Unread workspaces get a colored dot / badge on the name row
- Hover state: slightly lighter background

Colors should follow the user's terminal color scheme:
- Background: slightly lighter than terminal background
- Text: terminal foreground
- Branch: ANSI green
- Dirty branch: ANSI yellow
- PR info: ANSI cyan
- Ports: ANSI magenta
- Active border: ANSI blue (accent)

#### 3. Layout Integration (`wezterm-gui/src/termwindow/render/paint.rs`)

In `paint_pass()`:

```rust
// After painting tab bar, before painting panes:
let sidebar_width = if self.sidebar.visible { self.sidebar.width } else { 0.0 };

// Offset all pane positions by sidebar_width
// This is where pane pixel positions are calculated
for pos in &positioned_panes {
    pos.left += sidebar_width;
}

// Paint sidebar
if self.sidebar.visible {
    self.paint_sidebar(pass)?;
}
```

The critical function is `padding_left_top()` — it currently returns the pixel offset for the top-left corner of the pane area. Add `sidebar_width` to the left padding when visible.

#### 4. Resize Handling (`wezterm-gui/src/termwindow/resize.rs`)

When the sidebar opens/closes or its width changes, the terminal dimensions must be recalculated:

```rust
// Available width for terminal content:
let terminal_width = window_width - sidebar_width - scrollbar_width;
// Recompute pane grid dimensions
self.apply_dimensions(&dimensions, None, window);
```

The `resize_increment_calculator.rs` also needs to account for sidebar width when calculating the window size increments for pane grid alignment.

#### 5. Mouse Events (`wezterm-gui/src/termwindow/mouseevent.rs`)

Add `UIItemType::Sidebar` and `UIItemType::SidebarEntry(usize)` to the `UIItemType` enum:

```rust
pub enum UIItemType {
    // ... existing variants ...
    Sidebar,
    SidebarEntry { workspace_index: usize },
    SidebarNewButton,
}
```

Handle clicks:
- Click on `SidebarEntry` -> `mux.set_active_workspace(entry.workspace_name)`
- Click on `SidebarNewButton` -> prompt for workspace name, create it
- Right-click on `SidebarEntry` -> context menu (rename, close, move)

Handle hover:
- Update `sidebar.hovered` for highlight effect

#### 6. Data Polling (background thread)

Spawn a background thread that periodically refreshes workspace metadata:

```rust
fn poll_workspace_data(tx: Sender<Vec<WorkspaceEntry>>) {
    loop {
        let mux = Mux::get();
        let mut entries = Vec::new();

        for ws_name in mux.iter_workspaces() {
            let windows = mux.iter_windows_in_workspace(&ws_name);
            let active_pane = /* find focused pane in this workspace */;
            let cwd = active_pane.get_current_working_dir();

            entries.push(WorkspaceEntry {
                workspace_name: ws_name,
                cwd: cwd.clone(),
                git_branch: poll_git_branch(&cwd),
                git_dirty: poll_git_dirty(&cwd),
                pr_info: poll_pr_info(&cwd),       // less frequent
                listening_ports: poll_ports(&cwd),
                // notifications come from the notification store, not polling
                ..
            });
        }

        tx.send(entries).ok();
        std::thread::sleep(Duration::from_secs(3));
    }
}
```

The `TermWindow` receives updates via channel and rebuilds the sidebar `ComputedElement` tree.

#### 7. Configuration (`config/src/lib.rs`)

```lua
-- wezterm.lua
config.sidebar = {
    visible = true,         -- default: true
    width = 220,            -- pixels, default: 220
    position = "left",      -- "left" or "right"
    show_git = true,
    show_pr = true,
    show_ports = true,
    show_notifications = true,
    git_poll_interval = 3,  -- seconds
    pr_poll_interval = 30,  -- seconds
    port_poll_interval = 5, -- seconds
}
```

Add corresponding Lua API events:
- `format-sidebar-entry` — customize sidebar entry rendering (like `format-tab-title`)
- `sidebar-entry-clicked` — custom action on click

#### 8. Keybindings

| Key | Action | Description |
|-----|--------|-------------|
| `Cmd+B` | `ToggleSidebar` | Show/hide the sidebar |
| `Cmd+Shift+N` | `PromptNewWorkspace` | Create a new named workspace |
| `Cmd+Shift+E` | `ShowWorkspaceLauncher` | Quick-switch workspace (existing launcher) |
| `Cmd+1..9` | `ActivateWorkspace(n)` | Switch to nth workspace in sidebar order |
| `Cmd+Shift+U` | `JumpToUnreadNotification` | Focus the most recent unread pane (see Feature 2) |

New `KeyAssignment` variants in `config/src/keyassignment.rs`:
```rust
pub enum KeyAssignment {
    // ... existing ...
    ToggleSidebar,
    JumpToUnreadNotification,
}
```

---

## Feature 2: Notification System

### Notification Sources

1. **Terminal escape sequences** — parsed from the terminal output stream:
   - **OSC 9** (iTerm2/ConEmu): `\e]9;message\a`
   - **OSC 99** (kitty): `\e]99;i=id:d=0;body\a`
   - **OSC 777** (rxvt): `\e]777;notify;title;body\a`

2. **CLI tool** (`wezmux notify`):
   ```bash
   wezmux notify "Build complete"
   wezmux notify --pane-id 5 "Tests passed"
   wezmux notify --workspace backend "Deploy done"
   ```

3. **Agent hook integration** — example Claude Code hook:
   ```json
   {
     "hooks": {
       "notification": [
         { "event": "stop", "command": "wezmux notify 'Claude finished'" }
       ]
     }
   }
   ```

### Notification Store (`mux/src/notification.rs` — new file)

```rust
pub struct NotificationStore {
    /// All notifications, newest first
    notifications: Vec<Notification>,
    /// Per-pane unread state
    unread_panes: HashSet<PaneId>,
}

pub struct Notification {
    pub id: u64,
    pub pane_id: PaneId,
    pub workspace: String,
    pub title: String,
    pub body: String,
    pub timestamp: Instant,
    pub read: bool,
    pub source: NotificationSource,
}

pub enum NotificationSource {
    Osc9,
    Osc99,
    Osc777,
    Cli,
}

impl NotificationStore {
    /// Add a notification, mark pane as unread
    pub fn push(&mut self, notif: Notification);

    /// Mark all notifications for a pane as read (when user focuses it)
    pub fn mark_read(&mut self, pane_id: PaneId);

    /// Get the pane with the most recent unread notification
    pub fn most_recent_unread(&self) -> Option<PaneId>;

    /// Get latest notification text for a workspace (for sidebar display)
    pub fn latest_for_workspace(&self, workspace: &str) -> Option<&Notification>;

    /// Get unread count for a workspace
    pub fn unread_count(&self, workspace: &str) -> u32;
}
```

### OSC Parsing (`term/src/terminalstate/osc.rs` or equivalent)

WezTerm already parses many OSC sequences in the terminal state machine. The changes:

1. Find where OSC sequences are dispatched (likely in `termwiz/src/escape/osc.rs` or `term/src/terminalstate/`)
2. Add handlers for OSC 9, 99, 777 that emit a `MuxNotification::PaneNotification` event
3. The `Mux` receives this event and forwards to the `NotificationStore`

WezTerm may already handle some of these (check `OperatingSystemCommand` enum in termwiz). If so, hook into the existing handling and add the notification store push.

### Visual Indicators

#### Blue Ring on Panes

When a pane has unread notifications, draw a colored border around it:

In `wezterm-gui/src/termwindow/render/pane.rs`, after rendering pane content:

```rust
if notification_store.is_unread(pane_id) {
    self.paint_notification_ring(pane_position, accent_color)?;
}
```

The ring is a 2-3px border drawn as quads around the pane's bounding box, using the accent color (ANSI blue by default). This is similar to how split dividers are drawn in `render/split.rs` but as a hollow rectangle.

#### Sidebar Tab Indicator

In the sidebar `WorkspaceCard` rendering:

```
[ws name] [unread badge: blue dot or count]
```

The unread badge is a small filled circle or a count rendered next to the workspace name. When there are unread notifications, the entire card gets a subtle left-border accent or the name text gets the accent color.

#### Notification Panel (Cmd+I toggle)

A modal overlay (reuse `modal.rs` pattern) showing all notifications:

```
+-- Notifications ---------------------------+
| [workspace: backend] 2m ago               |
|   Claude finished - tests all passing     |
|                                            |
| [workspace: frontend] 5m ago              |
|   Build complete                          |
|                                            |
| [workspace: infra] 12m ago                |
|   Deploy to staging done                  |
+--------------------------------------------+
```

Rendered as a `ComputedElement` tree in a modal overlay, similar to the command palette (`palette.rs`) or character selector (`charselect.rs`).

### CLI Tool (`wezmux` binary — new crate)

Create a new crate `wezmux-cli/` in the workspace:

```rust
// wezmux-cli/src/main.rs
fn main() {
    let args = parse_args();
    match args.command {
        Command::Notify { message, pane_id, workspace } => {
            // Connect to wezterm mux server socket
            // Send a notification message via the codec
            let client = connect_to_mux();
            client.send(Notify { message, pane_id, workspace });
        }
    }
}
```

This uses WezTerm's existing client-server codec (`codec/` crate) to communicate with the running WezTerm instance. Add a new `Pdu` variant:

```rust
// codec/src/lib.rs
pub enum Pdu {
    // ... existing ...
    SendNotification {
        pane_id: Option<PaneId>,
        workspace: Option<String>,
        title: String,
        body: String,
    },
}
```

### Auto-Read Behavior

When the user focuses a pane (clicks on it, switches to it, or its workspace becomes active):
1. `NotificationStore::mark_read(pane_id)` is called
2. The blue ring disappears
3. The sidebar unread badge updates
4. This happens in `TermWindow::focus_pane()` or equivalent

---

## Implementation Phases

### Phase 1: Sidebar Shell (layout + empty sidebar)
**Files:** `sidebar.rs` (new), `paint.rs`, `resize.rs`, `mod.rs`, `mouseevent.rs`
- Add `SidebarState` to `TermWindow`
- Carve out horizontal space in the layout pipeline
- Render an empty sidebar panel with background color
- Handle resize when sidebar toggles
- Add `Cmd+B` keybinding for `ToggleSidebar`
- **Milestone:** sidebar panel appears/disappears, terminal content shifts

### Phase 2: Workspace List (static)
**Files:** `sidebar.rs`, `render/sidebar.rs` (new), `box_model.rs` (may need extensions)
- Build `ComputedElement` tree for workspace cards
- Render workspace name + active indicator
- Click to switch workspace
- Hover highlight
- `Cmd+Shift+N` to create new workspace
- **Milestone:** can see and switch between workspaces via sidebar

### Phase 3: Workspace Metadata
**Files:** `sidebar.rs` (polling logic), `render/sidebar.rs`
- Background thread polling git branch, dirty status
- Display cwd, git branch, dirty indicator in cards
- PR status polling (via `gh` CLI)
- Listening port detection
- **Milestone:** sidebar shows rich metadata like cmux

### Phase 4: Notification Store + OSC Handling
**Files:** `mux/src/notification.rs` (new), `term/` OSC handling, `mux/src/lib.rs`
- Create `NotificationStore`
- Hook OSC 9/99/777 parsing to push notifications
- Wire `MuxNotification` events
- Display latest notification text in sidebar
- Unread badge on sidebar entries
- **Milestone:** terminal programs can send notifications that appear in sidebar

### Phase 5: Visual Notification Indicators
**Files:** `render/pane.rs`, `render/sidebar.rs`
- Blue ring around unread panes
- Lit-up sidebar tab (accent border/color)
- Auto-mark-read on focus
- `Cmd+Shift+U` to jump to most recent unread
- **Milestone:** visual parity with cmux notification UX

### Phase 6: CLI + Notification Panel
**Files:** `wezmux-cli/` (new crate), `codec/`, modal overlay
- `wezmux notify` CLI tool
- New PDU in the codec for notifications
- Notification panel overlay (`Cmd+I`)
- **Milestone:** full notification system with CLI integration

### Phase 7: Polish
- Sidebar drag-to-reorder workspaces
- Sidebar context menu (rename, close workspace)
- Sidebar resize by dragging edge
- Smooth animations for sidebar show/hide
- Lua API: `format-sidebar-entry` event
- Configuration options in `config/`
- **Milestone:** production-ready UX

---

## Key Technical Risks

| Risk | Mitigation |
|------|------------|
| Box model engine may not support all needed layouts (scrolling, overflow) | The fancy tab bar already uses it successfully; worst case extend it |
| Git/PR polling may cause latency | Run in dedicated background thread, never block render |
| `gh` CLI may not be installed | Gracefully degrade — show "no PR info" |
| Cross-platform sidebar rendering differences | Test on macOS first (primary target), then Linux. Windows later |
| Notification store memory growth | Cap at ~1000 notifications, evict oldest |
| WezTerm's custom renderer makes UI work slower than native toolkit | Accept this tradeoff — the stability benefits justify the effort |

---

## Files to Create

| File | Description |
|------|-------------|
| `wezterm-gui/src/termwindow/sidebar.rs` | Sidebar state, data model, polling |
| `wezterm-gui/src/termwindow/render/sidebar.rs` | Sidebar rendering (ComputedElement tree) |
| `mux/src/notification.rs` | Notification store |
| `wezmux-cli/src/main.rs` | CLI tool |
| `wezmux-cli/Cargo.toml` | CLI crate manifest |

## Files to Modify

| File | Changes |
|------|---------|
| `wezterm-gui/src/termwindow/mod.rs` | Add `SidebarState` field, init, toggle |
| `wezterm-gui/src/termwindow/render/paint.rs` | Sidebar layout offset, `paint_sidebar()` call |
| `wezterm-gui/src/termwindow/render/mod.rs` | Add `mod sidebar;` |
| `wezterm-gui/src/termwindow/resize.rs` | Account for sidebar width |
| `wezterm-gui/src/termwindow/mouseevent.rs` | Add `UIItemType::Sidebar*`, handle clicks/hover |
| `config/src/lib.rs` | Sidebar config options |
| `config/src/keyassignment.rs` | `ToggleSidebar`, `JumpToUnreadNotification` |
| `mux/src/lib.rs` | `NotificationStore` integration, `MuxNotification` variant |
| `term/` or `termwiz/` (OSC handling) | Hook OSC 9/99/777 to notification store |
| `codec/src/lib.rs` | `SendNotification` PDU |
| `Cargo.toml` (workspace) | Add `wezmux-cli` member |
