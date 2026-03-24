# Project Research Summary

**Project:** wezmux — WezTerm fork with GPU-rendered sidebar and OSC notification system
**Domain:** GPU-accelerated terminal emulator fork (sidebar workspace manager + notification routing)
**Researched:** 2026-03-24
**Confidence:** HIGH

## Executive Summary

Wezmux is a fork of WezTerm that adds two features with no direct precedent in open-source terminal emulators: a persistent sidebar showing per-workspace metadata (git branch, PR status, listening ports), and an in-process notification system that routes OSC 9/777 sequences into visual indicators (blue pane rings, sidebar badges) rather than relying solely on OS toasts. The closest comparable product is cmux (Ghostty-based, macOS, proprietary), which demonstrates that this UX pattern is viable and valued by multi-agent developer workflows. The research consensus is clear: build on WezTerm's existing `box_model.rs` + `ComputedElement` rendering pipeline — the same system that already powers the fancy tab bar — rather than introducing any external UI framework.

The recommended approach is a strict six-phase build order driven by dependency constraints: sidebar shell first (Phase 1), then workspace list rendering (Phase 2), then background metadata polling (Phase 3), then the notification store and OSC parsing (Phase 4), then visual indicators (Phase 5), and finally CLI integration and the notification panel (Phase 6). This ordering is non-negotiable because downstream phases (badges, rings) are consumers of the notification store, and the notification store requires OSC parsing, which in turn requires the sidebar rendering infrastructure to already exist. Background polling must be architected off-thread from the very first commit — there is no safe "fix it later" path once blocking calls are embedded in subscriber callbacks.

The dominant risk is that this is a living fork of an active upstream project. The files most changed by this work (`box_model.rs`, `mouseevent.rs`, `osc.rs`) are also the files upstream modifies most frequently. Merge discipline (monthly rebases, minimal in-place edits, new files preferred over modifications) must be established at fork setup and treated as a first-class maintenance task, not deferred polish.

---

## Key Findings

### Recommended Stack

The stack is almost entirely dictated by the WezTerm codebase itself — this is a fork, not a greenfield project. The only genuinely new external dependency is `dashmap = "6.1"` for the workspace metadata cache. Everything else (`wgpu 25.0.2`, `glium 0.35`, `tokio 1.43`, `parking_lot 0.12`, `async-channel 2.3`, `euclid 0.22`) is already present in the workspace `Cargo.toml`. The critical implementation guide is `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` — the sidebar IS this pattern, and every implementation decision should start there. No GUI frameworks (egui, iced, ratatui) can be embedded; WezTerm draws all UI as GPU quads via its own pipeline.

**Core technologies:**
- `Rust (stable, 1.75+)`: implementation language — the fork is Rust; no choice
- `wgpu 25.0.2`: GPU rendering (WebGPU/Metal) — already in workspace; sidebar uses this pipeline
- `box_model.rs` / `ComputedElement`: CSS-like declarative layout engine — already powers fancy tab bar; sidebar reuses it directly
- `tokio 1.43`: async runtime for background polling — already the project's runtime; use `tokio::process::Command` for git/gh subprocesses
- `dashmap 6.1`: concurrent workspace metadata cache — only new dependency; allows render thread to read without blocking on poll writes
- `parking_lot::Mutex`: notification store synchronization — already in workspace; faster than `std::sync::Mutex` for short critical sections shared between sync render code and async poll code

### Expected Features

Research confirms a clear split between features users will expect on day one (table stakes) and features that differentiate wezmux from competitors. The feature dependency graph mandates that sidebar shell and notification store infrastructure be built before any visual indicators are possible.

**Must have (table stakes):**
- Workspace switching by keyboard (Cmd+1..9) and by sidebar click — foundational for any multiplexer
- Named workspaces with active highlight — WezTerm Mux already has workspace labels; visual layer required
- OSC 9 and OSC 777 notification support — widely used by Claude Code and other agent tools; already partially wired in termwiz
- Notification auto-clear on pane focus — expected lifecycle from cmux; users assume unread state is accurate
- Working directory display per workspace — prerequisite for all metadata polling
- Keyboard shortcut to jump to unread pane (Cmd+Shift+U) — essential navigation for multi-agent setups with many workspaces

**Should have (competitive differentiators):**
- Per-workspace git branch + dirty indicator in sidebar — no terminal emulator shows this without a shell prompt plugin
- Blue notification ring around panes with unread notifications — cmux's signature visual; immediately obvious which pane needs attention
- Unread notification badge on sidebar workspace cards — combines workspace view with unread state
- OSC 9/99/777 all three supported — other terminals pick one; supporting all means existing agent integrations work without reconfiguration
- Sidebar toggle (Cmd+B) — power users want maximum terminal real estate

**Defer (v2+):**
- PR status in sidebar (high complexity, `gh` CLI dependency, rate limiting concerns — validate core UX first)
- Port display via `lsof` polling (medium complexity; defer until metadata polling architecture is proven stable)
- Desktop notification passthrough via macOS `UNUserNotificationCenter` (code signing dependency; in-process rings + badges are primary path)
- Notification panel modal overlay (Cmd+Shift+U jump covers v1 navigation need)
- Custom notification command hook (personalization, not core)
- Sidebar drag-to-reorder, context menus, resize by dragging (polish; keyboard-first in v1)

### Architecture Approach

Wezmux adds two new modules (`wezterm-gui/src/termwindow/sidebar.rs`, `wezterm-gui/src/termwindow/render/sidebar.rs`), one new mux-layer module (`mux/src/notification.rs`), and targeted modifications to five existing files (`paint.rs`, `resize.rs`, `mouseevent.rs`, `mux/src/lib.rs`, OSC handler). The sidebar width is injected as additional left padding into `apply_dimensions()` — the cleanest integration that flows naturally into the existing pane resize pipeline. All git/gh/lsof subprocess calls happen exclusively on a dedicated background tokio task writing into a `DashMap`; the render thread only reads the latest snapshot. The `NotificationStore` lives in the `Mux` layer (not in `TermWindow`) so it is shared across multiple GUI windows.

**Major components:**
1. `SidebarState` (`sidebar.rs`) — visibility, width, cached workspace entries, background poller handle
2. `render/sidebar.rs` — builds `ComputedElement` tree from `SidebarState` and paints via `TripleLayerQuadAllocator`; cache invalidated only on data change, not every frame
3. `NotificationStore` (`mux/src/notification.rs`) — per-pane notification state, unread counts, capped at 1000 entries; lives in `Mux` for cross-window correctness
4. Background poller — `tokio::spawn` per workspace, `tokio::time::interval` driven, writes to `Arc<DashMap<String, WorkspaceMetadata>>`; never touches the render thread
5. Extended `UIItemType` enum — adds `SidebarEntry { workspace_index }` and `SidebarNewButton`; hit-testing is non-breaking additive change

### Critical Pitfalls

1. **PTY column count corruption from pixel-misaligned sidebar width** — after fixing sidebar pixel width, back-calculate actual used content area as `content_cols * cell_width_px` and let the sidebar absorb the fractional remainder. Wire this into Phase 1 before any PTY resize logic exists. Test with `tput cols` assertion before/after toggle.

2. **Blocking I/O inside Mux subscriber callbacks freezes the render loop** — subscriber callbacks execute synchronously on the main GUI thread. Any blocking call (git, gh, lsof) inside a callback produces 200-500ms render freezes. Callbacks must only enqueue work into a channel; all subprocess execution happens in a dedicated background task. Design this architecture before writing any subprocess logic.

3. **Sidebar toggle not sending SIGWINCH to running processes** — toggling the sidebar changes the content area width but does not automatically trigger PTY resize. Treat sidebar toggle as equivalent to a window resize event; explicitly call PTY resize for all visible panes. Test by running vim, toggling sidebar, asserting vim reflows.

4. **OSC 9 crashes unsigned macOS dev builds** — `UNUserNotificationCenter` requires code signing; unsigned dev builds produce `UNErrorDomain error 1` crashes. Design the notification system so the in-process store (rings, badges) is the primary path and OS toast is best-effort. Document `codesign --force --deep -s - WezTerm.app` for dev builds.

5. **Fork divergence makes upstream merges impractical** — the files most changed by this project (`box_model.rs`, `mouseevent.rs`, `osc.rs`) are also the files upstream modifies most. Establish the upstream remote and a monthly rebase cadence at fork setup. Prefer adding new files over modifying existing ones; when modification is unavoidable, isolate changes to clearly-delineated sections.

---

## Implications for Roadmap

Based on combined research, the feature dependency graph and architectural constraints dictate a specific build order. Phase ordering is determined by: (a) sidebar shell must precede all rendering, (b) notification store must precede all visual indicators, (c) background polling architecture must be established before any subprocess calls are written, and (d) PTY column alignment must be correct from the first sidebar implementation.

### Phase 0: Fork Setup and Upstream Discipline
**Rationale:** Fork drift (Pitfall 5) compounds exponentially if upstream tracking is not established at day zero. This is not optional polish.
**Delivers:** Clean fork from `main` HEAD (not the Feb 2024 tagged release), upstream remote configured, monthly rebase process documented, `dashmap = "6.1"` added to workspace `Cargo.toml`.
**Avoids:** Pitfall 5 (fork divergence), stale API surface from building against the 2024 tagged release instead of current `wgpu 25` / `tokio 1.43`.
**Research flag:** Standard patterns — no deep research needed. Steps are documented in STACK.md.

### Phase 1: Sidebar Shell
**Rationale:** Every subsequent phase requires the sidebar layout infrastructure. PTY column alignment (Pitfall 1) and SIGWINCH-on-toggle (Pitfall 3) must be correct before testing with real terminal apps in later phases.
**Delivers:** Empty sidebar panel appears/disappears on Cmd+B, terminal content area shifts correctly, `tput cols` assertion passes before and after toggle, startup cell-size race handled (Pitfall 6).
**Features:** Sidebar toggle (Cmd+B), fixed 220px default width, sidebar hidden by default.
**Files:** `sidebar.rs` (new), `render/paint.rs` (modify), `resize.rs` (modify), `mouseevent.rs` (modify), `config/keyassignment.rs` (modify).
**Avoids:** Pitfall 1 (PTY column corruption), Pitfall 3 (missing SIGWINCH), Pitfall 6 (startup cell size race).
**Research flag:** Standard patterns — `fancy_tab_bar.rs` is the reference implementation.

### Phase 2: Workspace List (Static)
**Rationale:** Users need functional workspace navigation before metadata is valuable. Click-to-switch and keyboard navigation are table stakes. Background polling (Phase 3) needs the workspace card rendering infrastructure to display its results.
**Delivers:** Sidebar shows workspace names, active workspace highlighted, click-to-switch works, new workspace button present, cwd displayed per workspace.
**Features:** Workspace switching by click and keyboard, active workspace highlight, new workspace button (Cmd+Shift+N), working directory display.
**Uses:** `box_model.rs` ComputedElement tree pattern from `fancy_tab_bar.rs`, `Mux::iter_workspaces()`, `UIItemType::SidebarEntry`.
**Avoids:** Anti-pattern of reconstructing ComputedElement every frame — cache in `SidebarState.computed`, invalidate only on data change.
**Research flag:** Standard patterns — well-documented in ARCHITECTURE.md.

### Phase 3: Workspace Metadata (Background Polling)
**Rationale:** Git branch, dirty indicator, and (future) port status require a background polling architecture. This architecture must be built correctly once — blocking I/O in subscriber callbacks (Pitfall 2) is the most dangerous anti-pattern in this codebase. Building it in a dedicated phase ensures the architecture is solid before PR status and port polling are layered on.
**Delivers:** Git branch and dirty indicator visible in sidebar workspace cards, background tokio task writing to `Arc<DashMap>`, render thread reading without blocking.
**Features:** Per-workspace git branch display, git dirty indicator, non-blocking metadata polling.
**Uses:** `tokio::process::Command`, `dashmap 6.1`, `tokio::time::interval`, channel-based result delivery.
**Avoids:** Pitfall 2 (blocking Mux subscriber callbacks), polling on the render thread, spawning subprocesses per-pane per-frame.
**Research flag:** Needs careful implementation review — the async/sync boundary between render thread and polling tasks is subtle. Reference `STACK.md` alternatives section for correct tokio patterns.

### Phase 4: Notification Store and OSC Parsing
**Rationale:** All visual indicators (Phase 5) are consumers of the notification store. The store must exist and be populated before any rings or badges can be rendered. OSC parsing hooks into existing termwiz infrastructure — do not rewrite the parser.
**Delivers:** `NotificationStore` in `mux/src/notification.rs`, OSC 9/777 dispatch hooking into `MuxNotification::PaneNotification`, per-pane unread tracking, store capped at 1000 entries.
**Features:** OSC 9 support, OSC 777 support, OSC 99 (kitty) if present in termwiz enum, per-pane unread count.
**Implements:** `NotificationStore` (mux layer), `MuxNotification::PaneNotification` variant.
**Avoids:** Pitfall 4 (OSC 9 crash on unsigned macOS builds — design in-process store as primary path), storing unsanitized OSC text (security pitfall — strip ANSI escapes before storing), re-implementing the OSC parser.
**Research flag:** Needs source verification — OSC 99 (kitty) may require a new termwiz enum variant; confirm before implementation.

### Phase 5: Visual Indicators
**Rationale:** Blue rings and sidebar badges are the user-facing payoff of the notification infrastructure. These are pure rendering changes that consume `NotificationStore` state — straightforward once Phase 4 is complete.
**Delivers:** Blue notification ring drawn around panes with unread notifications, unread badge on sidebar workspace cards, latest notification text preview in sidebar, auto-mark-read on pane focus, Cmd+Shift+U jump to most recent unread pane.
**Features:** Blue pane notification ring, sidebar unread badge, latest notification text, jump-to-unread (Cmd+Shift+U), auto-clear on focus.
**Avoids:** False "unread" rings on newly created panes (only set ring on notifications arriving after last focus), Cmd+Shift+U doing nothing silently (show status bar message when no unread).
**Research flag:** Standard patterns — rendering follows the same ComputedElement approach established in Phase 2.

### Phase 6: CLI Integration and Notification Panel
**Rationale:** The `wezmux-cli` crate and notification panel modal are standalone deliverables that depend on the full notification infrastructure from Phases 4-5. Deferred to last because they extend functionality rather than enabling it.
**Delivers:** `wezmux-cli` binary for external notification injection (useful for Claude Code and other agents), `SendNotification` PDU in codec, notification panel modal overlay (deferred from v1 per anti-features research).
**Features:** CLI notification injection, notification panel (Cmd+Shift+N modal).
**Avoids:** PR status polling and port display may be added here or deferred to v2 based on Phase 3 experience — do not commit to these in the roadmap until background polling architecture is proven stable.
**Research flag:** Needs research — `codec/src/lib.rs` PDU extension pattern is not covered in current research. Phase planning should include a research step.

### Phase Ordering Rationale

- Phase 1 before Phase 2: sidebar layout infrastructure must exist before workspace card rendering
- Phase 2 before Phase 3: workspace card rendering must exist before metadata rows can be added to cards
- Phase 2 before Phase 4: sidebar rendering must exist before notification badges can be displayed
- Phase 4 before Phase 5: notification store must be populated before rings and badges can consume it
- Phase 3 and Phase 4 can be sequenced in either order — they are independent of each other; Phase 3 first is recommended because background polling architecture patterns inform Phase 4 channel design
- Phase 6 last: depends on full notification infrastructure (Phases 4+5) and is standalone extension

### Research Flags

Phases needing deeper research during planning:
- **Phase 4:** OSC 99 (kitty) presence in termwiz `OperatingSystemCommand` enum requires source verification before implementation planning — may need a new variant
- **Phase 6:** `codec/src/lib.rs` PDU extension pattern for `SendNotification` is not covered in current research — needs a targeted research step when planning this phase

Phases with standard patterns (skip research-phase):
- **Phase 0:** Cargo workspace setup, upstream remote — standard git workflow
- **Phase 1:** `fancy_tab_bar.rs` is a complete reference implementation; sidebar follows the same pattern exactly
- **Phase 2:** `Mux::iter_workspaces()` and `UIItemType` extension are fully documented in ARCHITECTURE.md
- **Phase 5:** Pure rendering consuming notification store state; follows same ComputedElement patterns as Phase 2

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Primary sources: WezTerm `Cargo.toml` verified, `box_model.rs` API verified, `fancy_tab_bar.rs` render pattern verified, `UIItemType` extensibility verified, Mux singleton API verified |
| Features | MEDIUM-HIGH | cmux is the primary design reference but is proprietary; cmux feature set inferred from README + docs + HN discussion rather than source code. Core WezTerm features (workspaces, OSC support) are HIGH confidence from official docs |
| Architecture | HIGH | Based on WezTerm source analysis, DeepWiki docs generated from source, and DESIGN.md from project owner. Component boundaries and data flow diagrams derived from actual source structure |
| Pitfalls | HIGH | Most pitfalls verified via specific WezTerm GitHub issues (PTY race #563, OSC crash #5476, code signing NixOS #397866). Background polling constraint confirmed by DeepWiki architecture docs |

**Overall confidence:** HIGH

### Gaps to Address

- **OSC 99 (kitty) in termwiz:** Research confirms OSC 9 and OSC 777 are present in WezTerm's documented escape sequences and termwiz `OperatingSystemCommand` enum. OSC 99 is not in the official WezTerm escape sequence docs — needs source verification when beginning Phase 4. May require adding a new enum variant.

- **PR status polling rate limits:** The `gh pr view` integration is listed as a Phase 3 candidate but research flags rate limiting risk at 4+ workspaces. The safe approach is to defer PR status to v2 and validate the polling architecture with git-only polling first. The roadmapper should mark PR status as v2 unless the project owner explicitly prioritizes it.

- **Port detection on macOS:** `lsof -iTCP -sTCP:LISTEN` is documented as slow (~200ms) and global. The scoped alternative (`lsof -i -a -p <pid>`) requires knowing the pane's process group PID, which requires additional Mux API investigation. Defer port display to v2 alongside PR status.

- **`codesign` in CI/dev workflow:** The dev build + OSC 9 crash risk (Pitfall 4) requires either documenting ad-hoc signing (`codesign --force --deep -s -`) or ensuring the in-process notification path is completely independent of `UNUserNotificationCenter`. The roadmap should include a dev workflow setup task in Phase 0 or Phase 4.

---

## Sources

### Primary (HIGH confidence)
- WezTerm workspace `Cargo.toml` — dependency versions (tokio 1.43, wgpu 25.0.2, glium 0.35, async-channel 2.3)
- `wezterm-gui/src/termwindow/box_model.rs` — Element, ComputedElement, compute_element() API
- `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` — reference implementation for sidebar rendering pattern
- `wezterm-gui/src/termwindow/mouseevent.rs` — UIItem, UIItemType, resolve_ui_item() extensibility
- `mux/src/lib.rs` — Mux singleton, workspace API, MuxNotification variants
- DeepWiki: WezTerm GUI Frontend Architecture (3.1) — https://deepwiki.com/wezterm/wezterm/3.1-gui-frontend
- DeepWiki: WezTerm Multiplexer Architecture (2.2) — https://deepwiki.com/wezterm/wezterm/2.2-multiplexer-architecture
- WezTerm escape sequences — https://wezterm.org/escape-sequences.html (OSC 9, OSC 777 confirmed)
- WezTerm workspace API — https://wezterm.org/config/lua/wezterm.mux/set_active_workspace.html
- WezTerm GitHub issue #563 — PTY cell size race at startup
- WezTerm GitHub issue #5476 — OSC 9 toast delivery on macOS
- NixOS/nixpkgs issue #397866 — WezTerm code signing requirement for UNUserNotificationCenter
- cmux README and docs — https://github.com/manaflow-ai/cmux (primary design reference)

### Secondary (MEDIUM confidence)
- cmux notification docs — https://cmux.com/docs/notifications
- HN discussion on cmux — https://news.ycombinator.com/item?id=47079718 (community validation of feature value)
- DashMap 6.1.0 via docs.rs — concurrent HashMap recommendation for render/poll pattern
- Tokio shared-state docs — spawn_blocking vs tokio::process, DashMap recommendation
- Google AI Dev Forum — PTY width tied to chat panel causes output corruption (confirms Pitfall 1)
- Preset.io — fork drift analysis (confirms Pitfall 5 severity)

### Tertiary (LOW confidence)
- parking_lot vs std Mutex performance analysis (2025, multiple sources) — MEDIUM/LOW; directionally correct but specific numbers vary by workload

---
*Research completed: 2026-03-24*
*Ready for roadmap: yes*
