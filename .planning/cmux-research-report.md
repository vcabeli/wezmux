# cmux Research Report: Architecture & Feature Implementation

**Purpose**: Guide wezmux (WezTerm fork) implementation by understanding how cmux implements its killer features.

**Repository**: https://github.com/manaflow-ai/cmux
**Language**: Swift (native macOS app using AppKit + SwiftUI)
**Terminal Backend**: libghostty (Ghostty submodule)
**License**: AGPL-3.0

---

## 1. Architecture Overview

cmux is a **native macOS application** built in Swift using AppKit and SwiftUI. It is NOT Electron or Tauri. It embeds Ghostty's terminal renderer (libghostty) as a Git submodule and renders terminal content via the GPU-accelerated Ghostty engine.

**Key architectural components:**

| Component | Technology | File |
|-----------|-----------|------|
| App shell | SwiftUI + AppKit | `Sources/cmuxApp.swift`, `Sources/AppDelegate.swift` |
| Main window | SwiftUI split view | `Sources/ContentView.swift` |
| Terminal rendering | libghostty (Zig/C) | `Sources/GhosttyTerminalView.swift` |
| Sidebar | SwiftUI `VerticalTabsSidebar` | Built into GhosttyKit framework (closed-source portion) |
| IPC | Unix domain sockets | `Sources/TerminalController.swift` |
| CLI | Swift executable | `CLI/cmux.swift` |
| Shell integration | Zsh/Bash scripts | `Resources/shell-integration/` |

**Window layout** (from `ContentView.swift`):
- Left: `VerticalTabsSidebar` (configurable width, resizable)
- Center/Right: `WorkspaceContentView` with terminal/browser panels
- Top: Custom titlebar overlay (not a native toolbar)
- Sidebar width: min = `SessionPersistencePolicy.minimumSidebarWidth`, max = 1/3 of window

---

## 2. Claude Code Integration (THE killer feature)

### 2.1 Detection Mechanism

cmux does NOT detect Claude Code through process scanning or terminal output parsing. Instead, it uses a **wrapper script** that intercepts the `claude` CLI command.

**File**: `Resources/bin/claude`

The wrapper is a bash script placed on the PATH inside cmux terminals. When a user types `claude`, this wrapper runs instead of the real Claude binary. It:

1. Checks if `CMUX_SURFACE_ID` env var is set (proves we're inside cmux)
2. Pings the cmux socket (`cmux ping`, 0.75s timeout) to verify it's alive
3. If both conditions pass, **injects hooks** into Claude Code's `--settings` flag
4. If either fails, passes through to real Claude unchanged (graceful degradation)

### 2.2 Hook Injection

The wrapper injects **six lifecycle hooks** via Claude Code's `--settings` JSON:

```
Hook Name         | What it does                                        | Timing
------------------|-----------------------------------------------------|--------
SessionStart      | Registers session with cmux (maps session -> surface)| Sync, 10s timeout
Stop              | Fires completion notification with activity summary  | Sync, 2s timeout
SessionEnd        | Cleanup (catches Ctrl+C)                            | Sync, 2s timeout
Notification      | Forwards Claude's notifications to cmux sidebar     | Sync, 5s timeout
UserPromptSubmit  | Sets sidebar status to "Running"                    | Sync, 5s timeout
PreToolUse        | Clears "Needs input" status during tool execution   | ASYNC (non-blocking)
```

Each hook calls `cmux claude-hook <subcommand>` via the CLI, which communicates over the Unix socket to the running cmux app.

### 2.3 Session Mapping

**File**: `CLI/cmux.swift` (ClaudeHookSessionStore)

Sessions are persisted in `~/.cmuxterm/claude-hook-sessions.json`:
- Maps `session_id` -> `{workspaceId, surfaceId, cwd, pid}`
- Uses POSIX `flock` for concurrent access safety
- Auto-expires records older than 7 days
- The wrapper generates a UUID session ID via `uuidgen` (unless user passes `--resume` or `--session-id`)

### 2.4 Agent Status Display

The sidebar shows agent status through the **status entries** system:

- `UserPromptSubmit` hook -> sets status to "Running" (via `cmux set-status`)
- `PreToolUse` hook -> clears "Needs input" (async)
- `Stop` hook -> generates completion notification with summary
- `Notification` hook -> routes Claude's own notifications to the sidebar

**Status entries** are key-value pairs displayed as pills in the sidebar:
```swift
struct SidebarStatusEntry {
    let key: String      // e.g., "claude_code"
    let value: String    // e.g., "Running", "Idle", "Needs input"
    let icon: String?    // e.g., "sf:brain.head.profile" (SF Symbol)
    let color: String?   // hex color
    let url: URL?
    let priority: Int    // higher = displayed first
    let format: SidebarMetadataFormat  // .plain or .markdown
    let timestamp: Date
}
```

### 2.5 "Hi! How can I help?" Output Preview

cmux does NOT parse terminal buffer content to show Claude's greeting. The terminal output preview shown in the sidebar comes from **notification text**:
- Claude Code's `Notification` hook fires and sends the notification body to cmux
- The sidebar displays the **latest notification text** per workspace
- This is the mechanism for showing output preview - it's notification-driven, not terminal-buffer-driven

### 2.6 OSC Notification Suppression

When a workspace has an active Claude Code agent session (detected via `agentPIDs`), cmux **suppresses OSC desktop notifications** to prevent notification storms. The GhosttyTerminalView handler checks:
```
GHOSTTY_ACTION_DESKTOP_NOTIFICATION -> suppresses duplicates for
workspaces with active "claude_code" agent sessions
```

---

## 3. Sidebar Implementation

### 3.1 Rendering Technology

The sidebar (`VerticalTabsSidebar`) is a **SwiftUI view** that renders workspace cards in a vertical list. It is part of the GhosttyKit framework (not fully open-source), but the data model and APIs are visible.

### 3.2 Workspace Card Data

Each sidebar card displays these data fields:

| Field | Source | Polling Mechanism |
|-------|--------|-------------------|
| **Workspace name** | Terminal process title or custom name | Real-time via title OSC |
| **Current directory** | Shell integration `report_pwd` | Every prompt (precmd hook) |
| **Git branch** | Shell reads `.git/HEAD` directly | Every prompt + background watcher (1s during commands) |
| **Git dirty status** | `git status --porcelain` | Every prompt |
| **PR number & state** | `gh pr list` via shell integration | Every 45 seconds in background |
| **PR checks status** | `gh pr list` (PENDING/FAIL/PASS) | Same as PR polling |
| **Listening ports** | `lsof -nP -iTCP -sTCP:LISTEN` | Batched, burst scan (0.5s to 10s offsets) |
| **Agent status** | Claude hook -> `set-status` CLI | Real-time via hook callbacks |
| **Latest notification** | `TerminalNotificationStore` | Real-time via OSC / CLI |
| **Log entries** | `cmux log` CLI command | Real-time |
| **Progress bar** | `cmux set-progress` CLI command | Real-time |
| **Workspace color** | User-configurable hex color | Persistent config |
| **Pinned state** | User toggle | Persistent |
| **Unread count** | `TerminalNotificationStore` | Real-time |

### 3.3 Sidebar State API

The sidebar state is queryable via CLI:
```
cmux sidebar-state
```
Returns key-value pairs:
```
cwd=/Users/foo/project
git_branch=main dirty
pr=#123 open https://github.com/org/repo/pull/123
pr_label=PR
ports=8080,3000
status_count=1
progress=0.75 Building...
log_count=3
```

### 3.4 Sidebar Selection

Two modes (tracked in `SessionSidebarSnapshot`):
- `.tabs` - shows workspace list (default)
- `.notifications` - shows notification panel

Toggle via Cmd+I (notifications) or clicking sidebar sections.

### 3.5 Sidebar Width

- Resizable via drag handle between sidebar and terminal content
- Min width: configurable constant
- Max width: 1/3 of window width
- Persisted across sessions

---

## 4. Notification System

### 4.1 Notification Sources

Notifications come from THREE sources:

1. **OSC terminal sequences**: OSC 9, OSC 99, OSC 777 intercepted by libghostty via `GHOSTTY_ACTION_DESKTOP_NOTIFICATION` callback
2. **CLI command**: `cmux notify` sends a notification from any process
3. **Claude hooks**: `cmux claude-hook notification` routes Claude's notifications

### 4.2 Notification Data Model

```swift
struct TerminalNotification: Identifiable, Hashable {
    let id: UUID
    let tabId: UUID          // workspace ID
    let surfaceId: UUID?     // panel ID (optional)
    let title: String
    let subtitle: String
    let body: String
    let createdAt: Date
    var isRead: Bool
}
```

### 4.3 Notification Store Behavior

- In-memory storage indexed by tab ID and surface ID
- When a new notification arrives for a tab/surface, **clears prior notifications** for that tab/surface
- Checks app focus state:
  - If app focused: plays sound/custom command ("suppressed feedback")
  - If app not focused: schedules macOS `UNNotification`
- Optional workspace auto-reorder to front on notification (configurable)
- Dock badge with optional tag display

### 4.4 Blue Ring Indicators

**How notification rings render on panes:**

The `TerminalPanelView` passes `showsUnreadNotificationRing` to `GhosttyTerminalView`:

```swift
showsUnreadNotificationRing = hasUnreadNotification && notificationPaneRingEnabled
```

Two conditions:
1. `hasUnreadNotification` - data binding from notification store
2. `notificationPaneRingEnabled` - user preference via `@AppStorage`

The actual ring drawing is handled inside `GhosttyTerminalView` (the libghostty AppKit wrapper), not in SwiftUI. This means the ring is rendered as part of the terminal surface's GPU draw pass or as an NSView overlay.

### 4.5 Notification Panel UI

**File**: `Sources/NotificationsPage.swift`

- Vertical scroll list of notification rows
- Each row: unread dot (8x8 accent circle) + title + timestamp + body (3 lines max) + source tab + dismiss button
- "Clear All" button in header
- "Jump to Latest Unread" (Cmd+Shift+U)
- Auto-focuses first notification when opened

### 4.6 Custom Notification Sounds

Users can configure custom notification sounds (aif, aiff, caf, wav) or custom shell commands to execute on notification arrival.

---

## 5. Shell Integration & Data Polling

### 5.1 Communication: Socket-based IPC (NOT OSC)

cmux shell integration communicates via **Unix domain sockets**, not OSC sequences. The `_cmux_send()` function in zsh uses `zsh/net/unix` module for ~0.2ms sends. Payloads are fire-and-forget.

### 5.2 Shell Hooks

**Zsh** (`Resources/shell-integration/cmux-zsh-integration.zsh`):
- `preexec`: Detects git operations, starts HEAD file watching, triggers port scanning
- `precmd`: Syncs working directory, probes git branch/dirty status, manages PR polling
- `zshexit`: Cleanup

**Bash** (`Resources/shell-integration/cmux-bash-integration.bash`):
- `PROMPT_COMMAND`: After each command, reports state
- `PS0`: Before command execution (Bash 4.4+)

### 5.3 Data Reported to cmux

| Data | Socket Command | Trigger |
|------|---------------|---------|
| Working directory | `report_pwd` | Every prompt |
| TTY name | `report_tty` | Once per session |
| Git branch | `report_git_branch` | Every prompt + background watcher |
| Git dirty | Included with branch report | Every prompt |
| PR metadata | `report_pr` / `clear_pr` | Background poll every 45s |
| Port scan kick | `ports_kick` | Every ~10s |
| Shell state | `report_shell_state` | Command start/end |

### 5.4 Git Branch Detection (Optimized)

Shell integration reads `.git/HEAD` directly (no `git` subprocess!) for branch detection. During foreground command execution, a background watcher polls every 1 second. Full `git status --porcelain` runs at each prompt.

### 5.5 PR Detection

Background loop runs `gh pr list` (or `gh pr view`) every 45 seconds with a 20-second timeout. Extracts PR number, state (OPEN/MERGED/CLOSED), check status (PENDING/FAIL/PASS), and URL.

### 5.6 Port Detection

**File**: `Sources/PortScanner.swift`

Batched, coalesced scanning:
1. Panel calls `kick()` -> 200ms coalesce window
2. Burst of 6 scans at staggered offsets (0.5s, 1.5s, 3s, 5s, 7.5s, 10s)
3. Each scan: `ps -t <ttylist>` to map TTYs to PIDs, then `lsof -nP -iTCP -sTCP:LISTEN -Fpn` to find ports
4. Results joined by PID and mapped back to panels

---

## 6. Toolbar

**File**: `Sources/WindowToolbarController.swift`

The toolbar is minimal:
- A single custom NSToolbarItem showing the **focused command text** ("Cmd: git push")
- Uses `NSTextField` with 12pt medium system font, secondary label color
- Updates on title change, tab focus, or window activation
- Throttled at 30fps via `NotificationBurstCoalescer`

Note: The "bell, split buttons" mentioned in screenshots are likely part of the sidebar UI or window decorations (`WindowDecorationsController.swift`), not the NSToolbar.

---

## 7. Settings

**File**: `Sources/CmuxConfig.swift`

Settings are stored in `~/.config/cmux/cmux.json` (global) or `cmux.json` (local, found by directory traversal).

Config supports:
- Command definitions (name, description, restart behavior)
- Workspace layouts (splits, panes, surfaces)
- Surface types: terminal (with command, cwd, env) or browser (with url)
- File watching with auto-reload

Additional settings via `@AppStorage`:
- `notificationPaneRingEnabled` - toggle blue notification rings
- `sidebarTintColor` - separate light/dark mode colors
- Workspace placement (top, afterCurrent, end)
- Custom notification sounds

---

## 8. Key Takeaways for Wezmux Implementation

### What cmux gets right:
1. **Claude wrapper script is the key insight** - Don't try to detect Claude from terminal output. Wrap the `claude` binary and inject hooks via `--settings` JSON.
2. **Socket IPC, not OSC** - Shell integration uses Unix sockets for all sidebar metadata. OSC is only used for the standard notification sequences (9/99/777). Custom data flows through sockets.
3. **Status entries are the sidebar data model** - Key-value pairs with icon, color, priority. The sidebar is a list of workspace cards showing these entries.
4. **Batched, non-blocking polling** - Git, PR, and port scanning all happen in background with coalescing. The render thread never blocks.
5. **Notification suppression** - Active agent sessions suppress OSC notifications to prevent storms.

### Architecture differences for wezmux:
| cmux approach | wezmux equivalent |
|--------------|-------------------|
| SwiftUI `VerticalTabsSidebar` | `box_model.rs` Element tree (GPU-rendered) |
| Unix domain socket IPC | Can use existing Mux notification system + custom IPC |
| `@AppStorage` preferences | WezTerm's config system |
| `GhosttyTerminalView` ring rendering | `borders.rs` pane border color override |
| Shell integration scripts | Similar shell scripts, hook into WezTerm's shell integration |
| `cmux` CLI binary | Could be a separate binary or OSC-based protocol |

### Minimum viable feature set to replicate:
1. **Claude wrapper script** (`Resources/bin/claude` equivalent) that injects hooks
2. **Socket server** in wezmux that receives hook callbacks and shell reports
3. **Sidebar data store** (DashMap) with workspace metadata: git branch, PR, ports, agent status
4. **Shell integration** scripts that report cwd, git, ports via socket
5. **Notification store** triggered by OSC 9/99/777 + hook notifications
6. **Blue border rendering** on panes with unread notifications
7. **Sidebar UI** via box_model.rs showing workspace cards with metadata

### Critical implementation order:
1. Socket IPC server (foundation for everything)
2. Shell integration scripts (populates sidebar data)
3. Sidebar rendering with box_model.rs
4. Claude wrapper + hook system
5. Notification store + blue borders
6. Port scanning
