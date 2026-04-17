## Project

**Wezmux** — a WezTerm fork that adds cmux-inspired workspace management for AI agent workflows.

Persistent sidebar showing per-workspace metadata (git branch, PR status, ports, agent status, notifications) and an OSC-based notification system with visual indicators (blue rings on panes, sidebar badges). Built for managing multiple AI coding agents in a single terminal window.

**Core value:** See at a glance which workspace needs attention — the sidebar and notification rings make it obvious where work is happening and where input is needed.

**Platform:** macOS only (Cocoa + CGL/Metal backend).

## Build & Run

```bash
make install          # Release build → /Applications/Wezmux.app
make bundle           # Release build → target/Wezmux.app (dev, no overwrite)
make build            # Debug build (fast iteration)
make test             # Uses cargo nextest if available, falls back to cargo test
cargo build -p wezterm-gui   # Build just the GUI binary (fastest rebuild)
cargo test -p mux            # Test a specific crate
```

**IMPORTANT:** Always build and run `wezterm-gui`, never `wezterm`. The `wezterm` binary has a stale launch path that doesn't pick up changes.

`make install` produces a full `.app` bundle with codesigned binary, hooks, and the `bin/claude` wrapper.

## Architecture Overview

### Wezmux additions to WezTerm

| Component | Files | Purpose |
|-----------|-------|---------|
| **Sidebar** | `wezterm-gui/src/termwindow/sidebar.rs`, `render/sidebar.rs` | State management, metadata gathering, rendering |
| **Agent status** | `mux/src/agent_status.rs` | State machine: Working/Idle/NeedsInput per pane |
| **Notifications** | `mux/src/notification.rs` | Dedup store with unread tracking, capped at 1000 |
| **OSC 7777** | `wezterm-escape-parser/src/osc.rs`, `term/src/terminal.rs` | Custom escape sequence for structured agent status |
| **Workspace config** | `wezterm-gui/src/termwindow/workspace_config.rs` | Per-workspace display name, accent color, ordering |
| **Session persistence** | `mux/src/session.rs` | Save/restore workspaces, scrollback, sidebar cache |
| **Claude wrapper** | `bin/claude` | Auto-injects hooks into Claude Code via `--settings` |
| **Hook scripts** | `bin/hooks/on-*.sh` | Emit OSC 7777 + OSC 9 on agent lifecycle events |
| **PATH injection** | `mux/src/wezmux_zdotdir.rs` | Keeps `bin/` on PATH despite macOS `path_helper` |

### Data flow: Agent status

```
Hook script (bash)
  → printf '\033]7777;status;working\007' > /dev/tty
  → termwiz OSC parser (wezterm-escape-parser/src/osc.rs)
  → Alert::WezmuxStatus { event, data } (term/src/terminal.rs)
  → MuxNotification::Alert dispatch (mux/src/lib.rs:775-820)
  → AgentStatusStore.update_*() (mux/src/agent_status.rs)
  → generation counter increments
  → sidebar cache invalidated on next frame
  → render/sidebar.rs rebuilds card with new status
```

### Data flow: Notifications

```
Hook script → printf '\033]9;message\007' > /dev/tty
  → Alert::ToastNotification
  → notification_store.add_notification()
  → sidebar reads unread_count, shows badge
```

### Sidebar rendering pipeline

Uses WezTerm's `box_model.rs` (same as `fancy_tab_bar.rs`):

1. Build `Vec<Element>` tree (workspace cards, toolbar, new-workspace button)
2. `compute_element()` → pixel-accurate layout
3. `render_element()` → GPU quads
4. Extract `UIItem`s → hit-testing for clicks/drag

Sidebar width subtracts from terminal columns in `resize.rs`.

### Sidebar metadata gathering

Runs in a background thread, coalesced with 200ms delay:
- **Git branch/dirty**: `libgit2` (via `git2` crate) — reads `.git/HEAD` and `repo.statuses()`
- **PR status**: `gh pr view --json` subprocess — 60s refresh interval, degrades if `gh` missing
- **Listening ports**: `lsof -nP -iTCP -sTCP:LISTEN` subprocess
- **Agent status**: OSC 7777 (real-time, no polling)

Results cached in `SidebarState.metadata` HashMap. Persisted to session file for fast restore.

## The `bin/claude` wrapper

When Wezmux launches a shell, it prepends `bin/` to PATH. This puts the `bin/claude` wrapper ahead of the real `claude` binary.

The wrapper:
1. Detects if running inside Wezmux (`WEZMUX=1` env var)
2. If yes, injects all hook scripts via `--settings` JSON (Stop, Notification, UserPromptSubmit, PreToolUse, SubagentStart, SubagentStop)
3. If user already passed `--settings`, passes through unchanged
4. Outside Wezmux, passes through to real `claude` binary unchanged

**When adding new Claude Code hooks:** add the script to `bin/hooks/` AND add the entry to the settings JSON in `bin/claude`. No manual `~/.claude/settings.json` setup is needed — the wrapper handles everything automatically.

## OSC 7777 Protocol

```
ESC ] 7777 ; EVENT ; DATA BEL
```

| Event | Data | Effect |
|-------|------|--------|
| `status` | `working\|idle\|needs_input` | Updates agent status indicator |
| `message` | text | Sets preview text on workspace card |
| `tool` | tool name | Reports current tool (cached) |
| `subagents` | count (integer) | Shows "N background tasks" line |
| `clear` | (none) | Resets to idle, preserves last message |

State machine guards in `AgentStatusStore`:
- `needs_input` is sticky for 3 seconds (guards Stop/Notification hook race)
- `working` always overrides `needs_input` (user answered)
- Entering `working` clears message but keeps `last_working_message` as fallback

## Key modules

### `mux/src/agent_status.rs`
`AgentStatusStore` — HashMap<PaneId, AgentPaneStatus> with generation counter. Short `parking_lot::Mutex` critical sections (accessed from both sync render code and async poll code). 20 unit tests covering all state transitions.

### `mux/src/notification.rs`
`NotificationStore` — VecDeque with dedup, per-pane and per-workspace unread counts. Capped at 1000 entries.

### `wezterm-gui/src/termwindow/sidebar.rs`
`SidebarState` — metadata cache, refresh scheduling, agent detection via process tree scanning, cached entry invalidation via generation counters.

`WorkspaceEntry` — the data model for a single sidebar card: name, title, CWD, git info, PR, ports, agent info, notifications, accent color.

`build_agent_info()` — merges process detection + OSC 7777 data + cached agent type into `AgentInfo`.

### `wezterm-gui/src/termwindow/render/sidebar.rs`
`paint_sidebar()` — full render pipeline. `sidebar_entry_body_lines()` builds the line list for each card (agent status, background tasks, git branch, PR, ports).

### `wezterm-gui/src/termwindow/workspace_config.rs`
Per-workspace customizations persisted to `~/.config/wezmux/workspaces.json`. Display name, accent color (#RRGGBB), ordering. Atomic writes via temp file + rename.

### `mux/src/session.rs`
Session save/restore to `~/.local/share/wezterm/session/session.json`. Includes sidebar metadata cache, scrollback (zstd-compressed), window layout.

## Configuration

Lua config: `~/.wezterm.lua` or `~/.config/wezterm/wezterm.lua` (hot-reloaded).

Wezmux-specific config fields:
```lua
config.sidebar = {
  visible = true,        -- toggle: Cmd+B
  width = "280px",       -- Dimension (px, %, cell)
  colors = {
    bg = "#3a3a41",
    card_bg = "#303036",
    card_hover = "#44444b",
    accent = "#0091ff",
    text = "rgba(255,255,255,0.9)",
    muted = "rgba(255,255,255,0.55)",
    separator = "rgba(255,255,255,0.1)",
    pr_open = "#b860ff",
    pr_merged = "#4cc57c",
    pr_closed = "#d76a6a",
  },
}
```

## Environment variables

| Variable | Purpose |
|----------|---------|
| `WEZMUX=1` | Set by Wezmux in spawned shells; triggers hook injection in `bin/claude` |
| `WEZMUX_BIN` | Path to Wezmux `bin/` directory; prepended to PATH |
| `_WEZMUX_REAL_ZDOTDIR` | Original ZDOTDIR saved when PATH wrapper applied |
| `RUST_LOG=wezterm_gui=debug` | Debug logging for render/sidebar layer |

## Mouse interaction

Right-click on workspace card opens native context menu:
- Color picker (8 presets + reset)
- Move up/down/top/bottom
- Close workspace

Left-click switches workspace. Drag on right edge resizes sidebar.

Hit-testing via `UIItemType` variants: `SidebarWorkspace`, `SidebarNewWorkspace`, `SidebarResizeHandle`, `SidebarSplitHorizontal`, `SidebarSplitVertical`, `SidebarBackground`.

## Constraints

- **Renderer**: All UI drawn via WezTerm's custom GPU renderer — no native widgets, no egui/iced
- **Dependencies**: `gh` CLI may not be installed — PR status degrades gracefully
- **Performance**: Git/PR/port polling must never block the render thread (background thread + cache pattern)
- **Memory**: Notification store capped at 1000 entries
- **Hooks**: Each hook invocation is a separate process — no shared state between hook calls (use temp files for coordination, e.g. subagent counting)

## Conventions

- Follow existing WezTerm code style (rustfmt, clippy clean)
- New sidebar data: add field to `WorkspaceMetadata` or `AgentInfo` → `WorkspaceEntry` → `sidebar_entry_body_lines()` → render
- New OSC 7777 events: add match arm in `mux/src/lib.rs` dispatch + method in `AgentStatusStore`
- New hooks: add script to `bin/hooks/` + wire in `bin/claude` settings JSON
- Sidebar cache persistence: extend `SidebarCacheSerde` in `mux/src/session.rs`
- Tests go inline at bottom of module file
