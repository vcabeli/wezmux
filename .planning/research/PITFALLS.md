# Pitfalls Research

**Domain:** WezTerm fork — custom sidebar + notification system for GPU-rendered terminal emulator
**Researched:** 2026-03-24
**Confidence:** HIGH (most pitfalls verified via WezTerm GitHub issues, source architecture docs, and DeepWiki analysis)

---

## Critical Pitfalls

### Pitfall 1: Sidebar Width Not a Multiple of Cell Width Corrupts PTY Dimensions

**What goes wrong:**
The sidebar steals pixel columns from the total window width. When the remaining content area pixel width is not an exact multiple of the current cell width, the PTY column count reported via TIOCGWINSZ will be off by one column (or more). Programs that rely on the column count for line wrapping (shells, editors, TUI apps) will hard-wrap at the wrong position, producing corrupted output.

**Why it happens:**
WezTerm computes `columns = floor(content_width_px / cell_width_px)`. The sidebar is defined in pixels (e.g., 200px), but cell width is font-dependent (e.g., 10px, 11px, 13px — rarely a round divisor of 200). The remaining width `(window_px - sidebar_px)` will almost never be a clean multiple of cell width. This exact failure mode is documented in a Google AI developer forum where a chat-panel-width PTY caused hard-wrap line corruption.

**How to avoid:**
After the sidebar width is fixed (whether default or user-configured), compute `content_cols = floor((window_px - sidebar_px) / cell_width_px)`, then back-calculate the actual used content area: `content_px_used = content_cols * cell_width_px`. The sidebar absorbs the fractional remainder: `sidebar_actual_px = window_px - content_px_used`. This keeps PTY col count exact. Build this recalculation into the sidebar's layout pass and fire a SIGWINCH / PTY resize whenever the sidebar toggles or the window resizes.

**Warning signs:**
- Shell prompts wrapping at column 79 instead of 80 in an 80-column layout
- TUI apps (htop, vim, ncurses) drawing with one column of garbage on the right edge
- `tput cols` returning a value different from what the terminal displays

**Phase to address:** Phase 1 (Sidebar layout) — establish the cell-aligned width calculation before any PTY resize logic is wired up.

---

### Pitfall 2: Blocking Work Inside Mux Subscriber Callbacks Freezes the Render Loop

**What goes wrong:**
The Mux notification system (`notify()` / `notify_from_any_thread()`) invokes subscriber callbacks synchronously on the main GUI thread. If a subscriber for `PaneOutput`, `PaneFocused`, or a custom notification type does any blocking I/O — spawning `git status`, calling `gh pr view`, reading from the filesystem — the entire render loop stalls for the duration of that call. The terminal becomes unresponsive to keyboard input.

**Why it happens:**
The subscriber callback contract is synchronous. The DeepWiki architecture docs confirm: "Callbacks must complete quickly; long-running operations should spawn separate tasks via `spawn_into_main_thread()`." Developers see the subscriber as the natural place to "fetch the data I need," not realising the execution context.

**How to avoid:**
Subscriber callbacks must only enqueue work, never perform it. The correct pattern:
1. On subscriber trigger: push a message into a channel (`tokio::sync::mpsc` or a `crossbeam` channel).
2. A dedicated background `tokio::task` (or `std::thread`) drains that channel, runs the blocking subprocess (git, gh, lsof), and sends results back via `spawn_into_main_thread()` to update shared state.
3. After state update, call `window.invalidate()` to schedule a repaint.

Never spawn `Command::new("git")` or `Command::new("gh")` directly from any callback that runs on the main thread.

**Warning signs:**
- Terminal input lags when a workspace with git activity is focused
- Keyboard input feels "frozen" for 200-500ms periodically
- Profiling shows `notify()` call duration > 1ms

**Phase to address:** Phase 2 (Background metadata polling) — design the polling architecture before writing any git/gh/port subprocess logic.

---

### Pitfall 3: Sidebar Toggle Does Not Send SIGWINCH to Running Processes

**What goes wrong:**
When the sidebar is toggled open or closed, the GPU layout changes and the content area pixel width changes. If the PTY is not resized to match the new column count, running processes (shells, editors, pagers) continue operating at the old width. `vim` stays at 200 columns when sidebar opens and the true usable width drops to 160 columns; lines wrap into the sidebar region visually, or the editor draws off-screen.

**Why it happens:**
In WezTerm, pane PTY resize is tied to the geometry negotiation path that runs during window resize events. A sidebar toggle that only modifies the renderer's layout but doesn't propagate a dimension change to the PTY bypasses the resize path entirely. The terminal emulator layer does not automatically re-query available space.

**How to avoid:**
On every sidebar state change (open, close, width change), explicitly call the PTY resize API for all panes visible in the current workspace. In WezTerm's internals, this is `Pane::set_size()` or the equivalent pty resize call. Treat sidebar toggle as equivalent to a window resize event for the purposes of PTY notification. Write a test: toggle sidebar, check `tput cols` in a running shell, assert it changed.

**Warning signs:**
- Running shell prompts do not reflow after sidebar toggle
- `echo $COLUMNS` in a shell returns the pre-toggle value
- vim/neovim status bar overflows the visible area after toggle

**Phase to address:** Phase 1 (Sidebar layout) — wire PTY resize into the toggle action before Phase 3 testing with real TUI apps.

---

### Pitfall 4: OSC 9 Notifications Crash or Silently Fail on macOS Without Code Signing

**What goes wrong:**
WezTerm's existing OSC 9 / OSC 777 toast notification path on macOS calls `UNUserNotificationCenter`. If the binary is not code-signed (e.g., a dev build, a Nix/Homebrew-rebuilt fork), `UNUserNotificationCenter` returns `UNErrorDomain error 1` and either crashes the process or silently drops the notification. The in-process notification store (the custom state this fork adds) would still work, but the OS-level toast would fail.

**Why it happens:**
macOS requires app bundles to be code-signed for the User Notifications framework. WezTerm's NixOS package issue tracker confirms this: "The application must be code-signed for UNUserNotificationCenter to work." A fork built from source without a proper signing identity hits this wall immediately on macOS.

**How to avoid:**
For v1 (personal tool, macOS only), design the notification system to treat OS toast delivery as a best-effort side effect, not a required path. The in-process notification store (blue rings, sidebar badges) should function independently of OS notification delivery. If OS notifications are wanted, document that dev builds require ad-hoc signing (`codesign --force --deep -s - WezTerm.app`) to avoid crashes. Do not make any feature depend on the OSC 9 → OS toast path succeeding.

**Warning signs:**
- `printf "\e]9;test\e\\"` crashes WezTerm in dev builds
- Console.app shows `UNErrorDomain error 1` logs from WezTerm
- Notifications appear in the in-process store but never trigger an OS popup

**Phase to address:** Phase 3 (Notification store / OSC parsing) — test OSC 9 handling on both code-signed and ad-hoc-signed builds before shipping.

---

### Pitfall 5: Fork Divergence Compounds Over Time — Upstream Changes Become Unmergeble

**What goes wrong:**
WezTerm is an active project. The fork adds changes to `termwindow/render/`, `termwindow/mouseevent.rs`, `box_model.rs`, and `termwiz/src/escape/osc.rs`. Each upstream commit that touches those files creates a merge conflict. After 6 months of passive consumption, a rebase of the fork onto a newer upstream can represent weeks of work, or become impractical. Security fixes and major rendering improvements in upstream are stranded.

**Why it happens:**
The files modified by the sidebar and notification work are the same files upstream evolves most actively (renderer, mouse handling, escape parsing). "Fork drift" scales with the product of divergence depth × upstream velocity. WezTerm's upstream moves regularly; this fork will accumulate drift quickly if upstream merges are deferred.

**How to avoid:**
- Keep the diff minimal: prefer adding new files (e.g., `termwindow/sidebar/mod.rs`) over modifying existing large files. When modification of an existing file is unavoidable, isolate the change to a single clearly-delineated section guarded by a comment block.
- Merge or rebase from upstream at least monthly, treating it as a non-negotiable maintenance task rather than an optional cleanup.
- Use `git log --oneline upstream/main -- wezterm-gui/src/termwindow/` before each upstream merge to pre-identify conflict zones.
- Track upstream changes to `box_model.rs`, `mouseevent.rs`, and `osc.rs` specifically; these are the highest-risk files for this fork.

**Warning signs:**
- More than 2 weeks since last `git fetch upstream`
- `git diff upstream/main -- wezterm-gui/src/termwindow/box_model.rs` grows beyond 50 lines
- Upstream changelog mentions rendering pipeline changes

**Phase to address:** Phase 0 (Fork setup) — establish the upstream remote and monthly rebase cadence as part of the initial fork commit. Do not defer to later phases.

---

### Pitfall 6: Startup Cell Size Race — Sidebar Rendered Before Font Metrics Are Settled

**What goes wrong:**
WezTerm has a documented startup race (issue #563) where the initial PTY dimensions are calculated before the font/DPI metrics are fully settled. At startup, cells may be reported as 8x16px when the true rendered size is 10x22px. If the sidebar performs its initial layout pass during this window, the sidebar width in "columns" (for PTY alignment) will be calculated against wrong cell metrics. The first resize event corrects things, but initial rendering may be wrong.

**Why it happens:**
Font loading on macOS via Core Text is asynchronous relative to window creation. The first paint can occur before metrics stabilise. This is confirmed by the WezTerm maintainer: "initial pty dimensions to account for font/dpi size" was the fix, but early renders still use provisional metrics.

**How to avoid:**
Do not lock in the sidebar's cell-aligned width at the first paint call. Defer the definitive PTY resize to the first `window-resized` event or the first frame after the font metrics confirm (hook into the font-metrics-settled notification path if one exists, or use a one-shot `window-resized` handler at startup). Treat the first paint as provisional; the second paint after metrics settle is authoritative.

**Warning signs:**
- Sidebar appears slightly off-width on first launch, then snaps to correct width
- `tput cols` returns a different value before and after the first window resize
- HiDPI screens show more frequent startup glitches than 1x screens

**Phase to address:** Phase 1 (Sidebar layout) — explicitly test on HiDPI (Retina) macOS display at startup.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Poll git/PR/ports on a fixed timer regardless of workspace visibility | Simple implementation | CPU and subprocess churn for workspaces the user isn't looking at | Never — always gate polling on workspace active/visible state |
| Store notification history in a `Vec` without a size cap | Simple append | Unbounded memory growth in long-running sessions; PROJECT.md already caps at 1000 | Never — cap must be enforced from the first commit |
| Call `gh pr view` synchronously in the render path | Easy to prototype | Render stalls 200-500ms per workspace during PR fetch | Never — subprocess calls must always be off-thread |
| Hard-code sidebar width in pixels without cell-alignment recalculation | Fast first pass | PTY column count corruption; all TUI apps break | Never — cell alignment must be in the first layout implementation |
| Modify `box_model.rs` directly inline rather than via a new submodule | Less boilerplate | Every upstream merge to `box_model.rs` creates a conflict | Acceptable only if the change is a one-line hook point; prefer additive extension |
| Skip codesign on dev builds | Faster iteration | OSC 9 → UNUserNotificationCenter crashes on macOS | Acceptable if you document the crash and test with ad-hoc signing before release |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `gh pr view` | Calling it synchronously; assuming `gh` is installed | Run via `tokio::process::Command` in a background task; check `which gh` first and degrade gracefully if absent |
| `git rev-parse HEAD` / `git status` | Spawning per-pane on every render tick | Cache results with a TTL (e.g., 5s); invalidate on `PaneOutput` events from the watched pane, not on a timer |
| `lsof -i` for port detection | Global `lsof` is slow (~200ms) and lists all processes | Use `lsof -i -a -p <pid>` scoped to the pane's process group; or read `/proc/<pid>/net/tcp` (Linux) / use `netstat -anp` (macOS) scoped to PID |
| `Mux::subscribe()` callbacks | Performing any I/O inside the callback | Callbacks must only push to a channel; all work happens in a separate async task |
| UIItem registration | Registering items with stale coordinates from a previous frame | Re-register all `UIItem`s on every paint call; `ui_items` vector must be rebuilt each frame, not incrementally updated |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Spawning a new `git`/`gh` subprocess per workspace per poll tick | CPU spikes every N seconds; fan noise; battery drain | Coalesce polls: one background task per workspace, staggered with jitter; use `tokio::time::interval` with workspace-level deduplication | At 4+ workspaces open simultaneously |
| Full window repaint triggered by metadata update (git branch changed) | Frame rate drops; terminal feels sluggish during background polling | Only call `window.invalidate()` when sidebar data actually changed (compare old vs new before invalidating) | Immediately, if polling every 5s |
| `ui_items` hit-test scanning entire item list per mouse move event | Mouse interaction becomes sluggish as more sidebar entries are added | The existing `hit_test()` is already O(n) over items — acceptable at <50 items; no spatial index needed for v1 | At 50+ workspaces, becomes noticeable; irrelevant for v1 personal tool scope |
| Notification store holding full notification text for 1000+ entries | Memory growth; cloning notification store for render becomes expensive | Cap entries at 1000 (already in PROJECT.md); store only `(timestamp, pane_id, truncated_text[200])` — do not store full terminal output blobs | After ~1 month of heavy agentic use with frequent notifications |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Executing OSC 9 notification text as a shell command | Malicious terminal output could execute arbitrary commands | OSC 9 text must only be stored as a string and displayed; never passed to `Command::new("sh")` or similar |
| Displaying unsanitised OSC notification text in the sidebar | Terminal escape sequences in notification text could corrupt sidebar rendering | Strip all ANSI/OSC escape sequences from notification text before storing/displaying; use `strip-ansi-escapes` crate or equivalent |
| Running `gh pr view` with user-controlled `--repo` argument derived from git remote | A malicious git remote URL could inject shell arguments | Pass arguments as a Vec (not a shell string); use `Command::arg()` chaining, not `sh -c "gh pr view $(git remote ...)"` |

---

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Sidebar appears on startup before workspace metadata loads, showing blank/placeholder rows | First impression is a broken-looking sidebar | Show workspace names immediately (already in Mux); fill metadata cells with a subtle "loading" state, not empty strings |
| Blue notification rings appear on panes the user just created but never saw | False "unread" state is annoying | Only set the unread ring when a notification arrives *after* the pane was last focused, not on pane creation |
| Sidebar occupies width even when empty (no active workspaces beyond default) | Wastes screen space on first launch | Start with sidebar closed (`Cmd+B` to open); default state = hidden |
| `Cmd+Shift+U` to jump to most recent unread pane does nothing when there are no unread panes | Confusing — no feedback | Show a brief status-bar message: "No unread notifications" |
| Workspace click in sidebar switches workspace but does not focus the terminal | User clicks sidebar, types, and finds keystrokes go nowhere | After workspace switch, explicitly call `window.focus_pane()` on the active pane in the newly selected workspace |

---

## "Looks Done But Isn't" Checklist

- [ ] **Sidebar toggle:** Verify `tput cols` in a running shell changes correctly when sidebar opens/closes — not just that the sidebar renders
- [ ] **OSC 9 parsing:** Test with all three variants: `OSC 9 ; text ST`, `OSC 777 ; notify ; title ; body ST`, and the libnotify format — each has a different field structure
- [ ] **Notification auto-read:** Verify that switching *to* a pane via `Cmd+Shift+U` clears the ring, not just clicking on the pane manually
- [ ] **Background polling:** Confirm polling stops (or is rate-limited) when the WezTerm window is not focused — no subprocess spawns while the user is in another app
- [ ] **`gh` not installed:** Test the full PR status flow with `gh` removed from PATH — sidebar must show a degraded state, not crash or hang
- [ ] **Font change at runtime:** Change font size while sidebar is open — verify PTY columns recompute and sidebar re-renders at the new cell width
- [ ] **Upstream merge:** After first upstream merge, run the test suite and verify `box_model.rs`-derived layout still produces correct geometry

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| PTY column count corrupted by sidebar width misalignment | MEDIUM | Recalculate sidebar width using cell-aligned formula; add integration test that asserts `tput cols` before/after toggle; file as P0 bug |
| Main thread blocked by subscriber I/O | LOW-MEDIUM | Move blocking code into a `tokio::task::spawn_blocking` wrapper; the subscriber callback itself becomes a one-liner that sends a channel message |
| OSC 9 → crash on unsigned macOS binary | LOW | Add `codesign --force --deep -s - WezTerm.app` step to dev build docs; make in-process notification store the primary path and OS toast a secondary |
| Fork divergence makes upstream merge impractical | HIGH | Audit every changed file; extract your changes into clean patches; apply patches against new upstream base one by one; dedicate 2-3 days per year of accumulated drift |
| Startup cell size race causes wrong initial layout | LOW | Defer definitive layout to post-first-resize event; add `window-resized` one-shot handler that fires on first event and recalculates |
| `Mux::subscribe` callbacks accumulating stale entries | LOW | Ensure callbacks return `false` when the subscribing struct is dropped; audit for `Arc` cycles that keep subscriptions alive past their useful lifetime |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| PTY column corruption from sidebar pixel misalignment | Phase 1 (Sidebar layout) | `tput cols` assertion before and after sidebar toggle; test with multiple font sizes |
| Blocking I/O in Mux subscriber callbacks | Phase 2 (Background polling design) | Confirm render loop stays >30fps during a `gh pr view` call; run blocking-detector linter |
| Missing SIGWINCH after sidebar toggle | Phase 1 (Sidebar layout) | Run vim inside a pane, toggle sidebar, assert vim reflows |
| OSC 9 crash on unsigned macOS builds | Phase 3 (Notification store + OSC parsing) | Test on unsigned dev build; assert no crash when `printf "\e]9;test\e\\"` is executed |
| Fork divergence | Phase 0 (Fork setup) + ongoing | Monthly `git fetch upstream` + `git log upstream/main -- <high-risk files>` audit |
| Startup cell size race | Phase 1 (Sidebar layout) | Test on Retina display; check `tput cols` immediately after launch vs after first resize |
| OSC text containing escape sequences corrupting sidebar | Phase 3 (Notification store) | Send a notification containing `\e[31m` red ANSI code; assert sidebar renders as plain text |

---

## Sources

- WezTerm GitHub issue #563 — Cell size calculation not settled on startup: https://github.com/wezterm/wezterm/issues/563
- WezTerm GitHub issue #5476 — OSC 9 does not display toast, only in notification list: https://github.com/wezterm/wezterm/issues/5476
- NixOS/nixpkgs issue #397866 — WezTerm toast notifications fail or crash on macOS (code signing): https://github.com/NixOS/nixpkgs/issues/397866
- WezTerm GitHub issue #498 — OSC 777 & 9 hang terminal if dbus/notification daemon not running: https://github.com/wezterm/wezterm/issues/498
- WezTerm GitHub issue #4323 — Glitchy rendering when resizing: https://github.com/wezterm/wezterm/issues/4323
- WezTerm GitHub issue #3384 — Window does not re-render under WebGPU: https://github.com/wezterm/wezterm/issues/3384
- DeepWiki: WezTerm GUI Frontend Architecture (3.1): https://deepwiki.com/wezterm/wezterm/3.1-gui-frontend
- DeepWiki: WezTerm Multiplexer Architecture (2.2): https://deepwiki.com/wezterm/wezterm/2.2-multiplexer-architecture
- WezTerm docs — background_child_process (unconditional spawning warning): https://wezterm.org/config/lua/wezterm/background_child_process.html
- Google AI Dev Forum — PTY width tied to chat panel causes output corruption: https://discuss.ai.google.dev/t/bug-agent-terminal-pty-width-tied-to-chat-panel-causes-output-corruption/133781
- Preset.io — Fork drift in open source adoption: https://preset.io/blog/stop-forking-around-the-hidden-dangers-of-fork-drift-in-open-source-adoption/
- WezTerm macOS Integration (DeepWiki 4.4): https://deepwiki.com/wezterm/wezterm/4.4-macos-integration

---
*Pitfalls research for: WezTerm fork with GPU-rendered sidebar and OSC notification system*
*Researched: 2026-03-24*
