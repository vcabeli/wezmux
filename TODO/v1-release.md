# Wezmux Pre-v1.0 Release Checklist

Updated: 2026-04-02

Status: P0 complete. P1 has three open items blocking public v1.0.
Verification:
- `cargo test -p wezterm-gui workspace_config --bin wezterm-gui`
- `cargo test -p wezterm-gui sidebar --bin wezterm-gui`
- `cargo check -p wezterm -p wezterm-gui -p wezterm-mux-server -p strip-ansi-escapes`

## P0

- [x] Restore Wezmux-specific GUI tests so the sidebar/workspace surface compiles and passes under `cargo test -p wezterm-gui --bin wezterm-gui`.
- [x] Make the local `make test` entrypoint usable without requiring `cargo-nextest` to be preinstalled.
- [x] Align the primary public entrypoints around Wezmux:
  README, docs home, installation/help/config pages, issue-template contact links, and macOS permission strings.
- [x] Rename the public packaged metadata and shipped macOS assets that still presented the product as upstream WezTerm.
  Scoped to the current public release path: app metadata, desktop/appdata files, macOS artifact naming, Homebrew template, and docs release substitution.
- [x] Define the actual supported-platform matrix for Wezmux and make the docs match the shipped artifacts.
  Public `v1.0` support is macOS-only. Other platform paths remain inherited source/build internals until separately documented and released.
- [x] Unify release/version policy across git tags, crate/app versions, and changelog format.
  Public releases use semver with `vMAJOR.MINOR.PATCH` git tags, crate/app version `1.0.0`, and Wezmux-specific release notes.
- [x] Remove or replace the remaining upstream-only support, release, and funding references in the public docs, CI docs build path, and shipped metadata.

## P1

- [x] Fix config entrypoint inconsistency across inherited docs.
  Updated 10 files (13 edits) to say `~/.wezmux.lua` with `~/.wezterm.lua` fallback.
- [x] Ship dedicated Wezmux docs for the sidebar, OSC `7777`, notification flow, workspace metadata, and Claude/Codex integration hooks.
  Added: `docs/sidebar.md`, `docs/osc7777.md`, `docs/notifications.md`, `docs/agent-integration.md`.
- [ ] Reduce warning noise in key build targets so real regressions stand out in CI.
  `wezterm-gui` itself is clean (4 warnings). The `window` crate produces ~205 inherited warnings that drown CI output.

## Deferred (post-v1.0)

- Context menu: rename action (close/move/color all done)
- Workspace drag-reorder (context-menu move up/down works as interim)
- Lua extensibility: sidebar position, poll intervals, card layout options, notification behavior, `format-sidebar-entry` event, `sidebar-entry-clicked` event
- Dead code cleanup: `SidebarNotificationBell` and `SidebarSettings` enum variants and handlers (buttons removed from render path)

## Release Gate

Public `v1.0` should not ship until all P0 items are complete, the P1 config
and docs items are addressed, and the Wezmux-specific test targets pass in CI
for the supported macOS release path.
