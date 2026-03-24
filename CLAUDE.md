<!-- GSD:project-start source:PROJECT.md -->
## Project

**Wezmux**

A fork of WezTerm that adds cmux-inspired workspace management: a persistent sidebar showing per-workspace metadata (git branch, PR status, ports, notifications) and an OSC-based notification system with visual indicators (blue rings on panes, sidebar badges). Built as a personal tool for managing multiple agent workspaces in a single terminal window.

**Core Value:** See at a glance which workspace needs attention — the sidebar and notification rings must make it obvious where work is happening and where input is needed.

### Constraints

- **Platform**: macOS only for v1 (Cocoa + CGL backend)
- **Renderer**: All UI must be drawn via WezTerm's custom renderer — no native widgets available
- **Dependencies**: `gh` CLI may not be installed — PR status must degrade gracefully
- **Performance**: Git/PR/port polling must never block the render thread
- **Memory**: Notification store capped at ~1000 entries to prevent unbounded growth
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Recommended Stack
### Core Technologies
| Technology | Version (in WezTerm) | Purpose | Why Recommended |
|------------|----------------------|---------|-----------------|
| Rust (stable) | 1.75+ (inferred from wgpu 25) | Implementation language | The fork is Rust; no choice here. |
| wgpu | 25.0.2 | GPU rendering backend (WebGPU/Metal/OpenGL abstraction) | Already used by WezTerm for its WebGPU front end. The sidebar must be drawn through this renderer — no native widget toolkit available. |
| glium | 0.35 | OpenGL rendering backend (legacy path) | WezTerm supports both OpenGL (via glium) and WebGPU (via wgpu). The sidebar render path must work with both. |
| euclid | 0.22.x | 2D geometry primitives (Point2D, Rect, Size2D) | Used throughout wezterm-gui for all coordinate math and layout rects. Required for `compute_element()` layout context. |
| tokio | 1.43 | Async runtime for background polling tasks | Already the project's async runtime. Used for all I/O: git status polling, `gh` CLI subprocess calls, port scanning. |
### Existing WezTerm Subsystems to Reuse (NOT External Dependencies)
| Module | Location in Codebase | Purpose | Why Reuse (Not Replace) |
|--------|----------------------|---------|-------------------------|
| `box_model.rs` / `Element` / `ComputedElement` | `wezterm-gui/src/termwindow/box_model.rs` | CSS-like declarative layout engine | Powers the `fancy_tab_bar` already. Supports padding/margin/borders/colors/children. `compute_element()` resolves to pixel rects; `render_element()` emits draw calls. The sidebar uses the exact same builder pattern. |
| `fancy_tab_bar.rs` render pattern | `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` | Reference implementation for box-model UI | The sidebar IS this pattern. Build element tree → `compute_element(&LayoutContext{..}, &root)` → `translate()` → `paint_*` → extract `ui_items()` for hit-testing. Study this file before writing a line of sidebar code. |
| `UIItem` / `UIItemType` | `wezterm-gui/src/termwindow/mouseevent.rs` | Hit-region registration and mouse dispatch | The sidebar adds a new `UIItemType::Sidebar(SidebarItem)` variant. `TermWindow.ui_items: Vec<UIItem>` is populated each render frame; `resolve_ui_item()` tests clicks. |
| `Mux` singleton | `mux/src/lib.rs` | Workspace and pane registry | `Mux::get()` for global access. `iter_workspaces()`, `iter_windows_in_workspace()`, `active_workspace()`, `set_active_workspace()`. Subscribe to `MuxNotification::ActiveWorkspaceChanged`, `PaneOutput`, `WindowCreated` etc. for reactive updates. |
| `termwiz` OSC parser | `termwiz/src/escape/` | Escape sequence parsing | OSC 9 ("toast") and OSC 777 (`notify;title;body`) are already partially wired. The notification store hooks into this parser output — do NOT write a second OSC parser. |
| `TermWindow` render loop | `wezterm-gui/src/termwindow/mod.rs` | Main render orchestration | Add sidebar rendering calls inside `do_paint_webgpu()` / `do_paint()`. The window layout calculation (content area shift) happens in `resize.rs`. |
### Supporting Libraries (External Crates to Add)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `parking_lot` | 0.12.x (already in wezterm-gui deps) | `Mutex` and `RwLock` for notification store | Use `parking_lot::Mutex` — it is 1.5–5x faster than `std::sync::Mutex` under contention, uses 1 byte, and is already a project dependency. The notification store (pane_id → Vec<Notification>) needs this. |
| `dashmap` | 6.1.0 (stable) | Concurrent `HashMap` for metadata cache | Use for the workspace metadata cache (git branch, PR status, port list per workspace). Sharded locking allows polling threads to write individual workspace entries without blocking the render thread reading all entries. Do not use `Arc<RwLock<HashMap>>` — DashMap is the standard recommendation for this pattern. |
| `async-channel` | 2.3 (already in workspace) | Unbounded MPSC from poll workers to notification store | Already in the workspace. Use `async_channel::bounded()` to deliver poll results back to the main async context without blocking. |
### Development Tools
| Tool | Purpose | Notes |
|------|---------|-------|
| `cargo build --package wezterm` | Build the GUI binary | The workspace root builds all crates; `--package wezterm` targets just the terminal binary. |
| `RUST_LOG=wezterm_gui=debug` | Runtime logging during development | WezTerm uses the `log` crate throughout. Set this env var to see render-layer trace output. |
| `cargo test -p wezterm-gui` | Unit tests for layout logic | The `box_model` and `UIItem` systems are unit-testable in isolation. Test sidebar layout math here. |
| `cargo clippy --package wezterm-gui -- -D warnings` | Lint on every change | WezTerm's CI enforces clippy; keep the fork passing. |
## Installation
# Fork setup
# Add dashmap (only new external dependency)
# In wezterm-gui/Cargo.toml, add:
# dashmap = { workspace = true }
# In workspace Cargo.toml [workspace.dependencies], add:
# dashmap = "6.1"
# Build
# Run
## Alternatives Considered
| Our Choice | Alternative | Why Not |
|------------|-------------|---------|
| `box_model.rs` + `ComputedElement` for sidebar layout | Build a new layout engine from scratch | `fancy_tab_bar.rs` already proves this system works for panel-style UI. Building a second layout engine doubles maintenance surface and risks rendering inconsistencies. |
| `box_model.rs` + `ComputedElement` for sidebar layout | Ratatui or similar TUI framework | Ratatui targets character-grid terminals; WezTerm's renderer works at GPU quad level, not character level. Cannot mix the two systems — WezTerm draws ALL UI itself. |
| `dashmap` for workspace metadata cache | `Arc<RwLock<HashMap>>` | DashMap's sharded locking avoids the single-lock bottleneck. Tokio's own docs recommend DashMap when HashMap access is frequent and fine-grained. |
| `tokio::spawn` + `tokio::time::interval` for polling | A dedicated polling thread with `std::thread::sleep` | WezTerm is already tokio-runtime. Spawning std threads just to sleep introduces unnecessary overhead and breaks structured concurrency. Use `tokio::spawn(async { loop { interval.tick().await; ... } })`. |
| `tokio::process::Command` for git/gh subprocess | `std::process::Command` in `spawn_blocking` | `tokio::process::Command` is non-blocking natively. `spawn_blocking` is appropriate only for truly CPU-bound sync code. Subprocess I/O is I/O-bound — use the async process API. |
| `parking_lot::Mutex` for notification store | `tokio::sync::Mutex` | The notification store is accessed from synchronous render code and async poll code. `tokio::sync::Mutex` must only be held across `.await` points. The store's critical sections are short and synchronous — `parking_lot::Mutex` is correct and faster. |
## What NOT to Use
| Avoid | Why | Use Instead |
|-------|-----|-------------|
| **egui, iced, or any GUI framework** | WezTerm's renderer is a custom GPU pipeline. It does not embed a foreign widget system. Every pixel is drawn via quads/glyphs by `TermWindow`. Adding a second UI library would require either embedding a foreign OpenGL/wgpu context (nightmare) or rendering to a texture and blitting (unnecessary complexity). | `box_model.rs` + `Element` builder pattern — already in the codebase. |
| **`std::sync::mpsc` for poll-to-render communication** | Standard `mpsc` blocks on `recv()`, which would stall the render thread. | `async-channel::bounded()` (already in workspace) or `tokio::sync::mpsc`. |
| **Polling on the render thread** | Git/`gh`/port commands can take 200ms+. The render thread targets 60fps (16ms frames). Any blocking call will cause visible stuttering. | `tokio::spawn` background tasks that write results into a `DashMap`. The render thread only reads the cache; it never waits for I/O. |
| **`dashmap` v7.0.0-rc2** | Release candidate — API may break. Version 6.1.0 is the current stable. | `dashmap = "6.1"` |
| **Forking from the `20240203` tagged release** | The Feb 2024 release is the last tagged release but the `main` branch has ~14 months of additional commits. WezTerm uses nightly builds from `main` in practice. Fork from `main` HEAD to get current wgpu 25, tokio 1.43, and render fixes. | `git clone` from `main` branch. |
| **Re-implementing OSC parsing** | `termwiz/src/escape/` already handles OSC 9 and OSC 777. Writing a parallel parser creates a second code path that will diverge. | Hook into the existing `MuxNotification::PaneOutput` subscription and intercept OSC events as they flow through the existing parser. |
## Stack Patterns by Variant
- Build a `Vec<Element>` representing workspace rows using the `Element::new(&font, ElementContent::Children(children))` pattern from `fancy_tab_bar.rs`
- Call `compute_element(&layout_ctx, &root_element)` to get pixel-accurate `ComputedElement`
- Call `render_element(...)` to emit draw calls
- Extract `ui_items()` from `ComputedElement` and push into `TermWindow.ui_items`
- Modify `wezterm-gui/src/termwindow/render/borders.rs` — this file already handles pane border drawing
- Add a check: if `notification_store.has_unread(pane_id)`, override border color to blue
- Keep the check O(1) via DashMap lookup — do not iterate the store during render
- One `tokio::spawn` per workspace, refreshed every N seconds via `tokio::time::interval`
- `tokio::process::Command::new("git")` with `output().await` — non-blocking
- Write results to `Arc<DashMap<String, WorkspaceMetadata>>`
- `TermWindow` reads the DashMap on each render frame — no locking stalls
- `UIItemType::Sidebar(SidebarClick::WorkspaceEntry(name))` → `mouse_event_ui_item()` handler
- Call `Mux::get().set_active_workspace(&name)` — this triggers `MuxNotification::ActiveWorkspaceChanged`, which the GUI already handles for tab switching
## Version Compatibility
| Package | Compatible With | Notes |
|---------|-----------------|-------|
| `wgpu = "25.0.2"` | `tokio = "1.43"` | Both in the workspace; no conflicts. |
| `dashmap = "6.1"` | `parking_lot = "0.12"` | DashMap 6.x uses parking_lot 0.12 internally for shard locks — versions must match or DashMap will compile its own. Pin workspace to `parking_lot = "0.12"`. |
| `async-channel = "2.3"` | `tokio = "1.43"` | async-channel 2.x is runtime-agnostic; works with tokio without feature flags. |
| `euclid = "0.22"` | All versions above | Geometry-only crate with no async dependencies. |
## Sources
- WezTerm workspace `Cargo.toml` (verified): tokio 1.43, wgpu 25.0.2, glium 0.35, async-channel 2.3, crossbeam 0.8 — HIGH confidence
- `wezterm-gui/src/termwindow/box_model.rs` (verified via GitHub fetch): `Element`, `ComputedElement`, `compute_element()`, builder API — HIGH confidence
- `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` (verified): `BoxDimension`, element tree construction, `compute_element()` render pipeline — HIGH confidence
- `wezterm-gui/src/termwindow/mouseevent.rs` (verified): `UIItem`, `UIItemType` variants, `resolve_ui_item()`, extensibility via enum — HIGH confidence
- `mux/src/lib.rs` (verified): `Mux::get()` singleton, workspace API, `MuxNotification` variants — HIGH confidence
- `wezterm.org/escape-sequences.html` (verified): OSC 9 and OSC 777 notification support — HIGH confidence
- `github.com/wezterm/wezterm/issues/6341` (verified): Project is active, nightly builds continue from `main`, last tagged release Feb 2024 but main has 14+ months of commits — HIGH confidence
- DashMap 6.1.0 (verified via search + docs.rs): Latest stable; v7.0.0-rc2 exists but is pre-release — HIGH confidence
- Tokio shared-state docs (official): `spawn_blocking` vs `tokio::process`, DashMap recommendation — HIGH confidence
- parking_lot vs std Mutex analysis (2025, multiple sources): `parking_lot` recommended for contended short critical sections — MEDIUM confidence
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd:quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd:debug` for investigation and bug fixing
- `/gsd:execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd:profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
