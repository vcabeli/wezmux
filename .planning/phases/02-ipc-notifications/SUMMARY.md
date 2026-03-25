# Phase 2: IPC Notifications — OSC 7777 Structured Agent Status Protocol

## What we did

Replaced fragile buffer scanning and string matching with a structured escape sequence protocol for communicating agent status to the sidebar.

### Before
- **Buffer scanning**: `read_terminal_preview()` polled 50 lines of terminal buffer every 200ms with 20+ heuristic filters to skip UI chrome (Zellij bars, model names, keybinding hints, etc.)
- **String matching**: `infer_agent_status()` keyword-matched notification text ("waiting for" -> NeedsInput, "finished" -> Idle, etc.)
- Fragile, slow (200ms polling), and couldn't show Claude's actual response text

### After
- **OSC 7777 protocol**: `\033]7777;<event>;<data>\007`
  - `status;working|idle|needs_input` — agent lifecycle
  - `message;<text>` — preview text (Claude's actual response)
  - `tool;<name>` — currently executing tool
  - `clear` — reset all status
- Hook scripts emit structured events on agent lifecycle transitions
- Status updates are instant (event-driven, no polling)
- Claude's actual response text shown via `last_assistant_message` from Stop hook

## Files changed

### New
- `mux/src/agent_status.rs` — Per-pane AgentStatusStore with status, message, tool, 10-min expiry, generation counter, unit tests

### Parser pipeline (OSC 7777 plumbing)
- `wezterm-escape-parser/src/osc.rs` — `WezmuxStatus = "7777"` code, enum variant, parse arm, Display impl
- `term/src/terminal.rs` — `Alert::WezmuxStatus` variant
- `term/src/terminalstate/performer.rs` — Dispatch handler routing OSC 7777 to alert system

### Mux routing
- `mux/src/lib.rs` — `agent_status` module, store field on Mux, WezmuxStatus alert routing, `agent_status_for_pane()` and `agent_status_generation()` accessors

### Sidebar
- `wezterm-gui/src/termwindow/sidebar.rs`
  - **Deleted**: `read_terminal_preview()`, `read_terminal_preview_inner()` (~120 lines of buffer scanning)
  - **Deleted**: `infer_agent_status()` (~30 lines of string matching)
  - **Deleted**: `terminal_previews` HashMap cache, `SIDEBAR_PREVIEW_REFRESH_INTERVAL`, `terminal_previews_updated`
  - **Modified**: `build_agent_info()` reads from AgentStatusStore; shows agent card when OSC 7777 data exists (no process detection required)
  - **Added**: Cache invalidation on agent status generation changes, metadata changes, and cwd changes
- `wezterm-gui/src/termwindow/render/sidebar.rs` — Removed `terminal_preview` references; agent status message falls back to `latest_notification`; PR icons use Octicon Nerd Font glyphs (git_pull_request, git_merge, git_pull_request_closed)

### GUI plumbing
- `wezterm-gui/src/frontend.rs` — Added `WezmuxStatus` to Alert match arms
- `wezterm-gui/src/termwindow/mod.rs` — `WezmuxStatus` in alert dispatch; `CurrentWorkingDirectoryChanged` invalidates sidebar cache
- `wezterm-gui/src/termwindow/render/paint.rs` — `paint_sidebar` errors now propagate to the retry loop (fixes sidebar disappearing on texture atlas exhaustion)
- `wezterm-gui/src/main.rs` — Sets `WEZMUX=1` env var and prepends `bin/` to PATH so child shells automatically use the claude wrapper

### Hook scripts
- `bin/hooks/on-prompt-submit.sh` — Emits `OSC 7777;status;working` + `OSC 9`
- `bin/hooks/on-stop.sh` — Extracts `last_assistant_message` from JSON, emits as `OSC 7777;message`, plus `status;idle` + `OSC 9`. Strips BEL/ESC to prevent OSC injection.
- `bin/hooks/on-notification.sh` — Emits `OSC 7777;message` + `OSC 9`. Strips BEL/ESC to prevent OSC injection.
- `bin/hooks/on-needs-input.sh` — Emits `OSC 7777;status;needs_input` + `OSC 9`
- `bin/claude` — Added `PreToolUse` hook for `AskUserQuestion`

## Design decisions

1. **Dual emit (OSC 7777 + OSC 9)**: Hooks emit both because OSC 7777 feeds the agent status store (structured status) while OSC 9 feeds the notification store (unread counts, badge indicators). Both are needed.

2. **Agent detection without process info**: If the OSC 7777 store has data for a pane, the sidebar shows agent info even without process detection. This means any tool that emits OSC 7777 gets sidebar support.

3. **Generation counter for instant invalidation**: The AgentStatusStore bumps a generation counter on every mutation. The sidebar cache compares this to skip re-rendering when nothing changed, but detects changes instantly when they occur.

4. **`last_assistant_message`**: The Claude Code Stop hook receives the full assistant response in JSON. We truncate to 200 chars and emit as the preview message — replacing buffer scanning entirely.

5. **`update_message` creates entries**: If a message arrives before a status event (e.g. Notification hook fires first), the store creates an entry defaulting to Working status rather than silently dropping the message.

6. **Automatic env setup**: Wezmux sets `WEZMUX=1` and prepends `bin/` to PATH at startup. Child shells inherit these, so the `bin/claude` wrapper activates automatically — no user aliases or config needed.

## Net impact
- **Removed**: ~170 lines of buffer scanning heuristics
- **Added**: ~130 lines of structured protocol + store
- **Performance**: No more 200ms polling of terminal buffers across all workspaces; status updates are event-driven and instant
- **Reliability**: No more false positives from heuristic filters; status comes from structured data
- **Security**: Hook scripts sanitize control characters to prevent OSC escape injection
- **Stability**: Sidebar texture atlas exhaustion triggers retry/resize instead of silent disappearance
