# Wezmux Pre-v1.0 Release Checklist

Updated: 2026-03-31

Status: P0 complete for the public macOS `v1.0` release path.
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

- [ ] Decide whether `~/.wezmux.lua` or `~/.wezterm.lua` is the single primary public config entrypoint, then make every doc and template say the same thing.
- [ ] Ship dedicated Wezmux docs for the sidebar, OSC `7777`, notification flow, workspace metadata, and Claude/Codex integration hooks.
- [ ] Close or consciously defer visible UI backlog before launch:
  notification panel, bell action, settings action, context menu polish, workspace reordering, and Lua extensibility points.
- [ ] Reduce warning noise in key build targets so real regressions stand out in CI.

## Release Gate

Public `v1.0` should not ship until all P0 items are complete and the
Wezmux-specific test targets pass in CI for the supported macOS release path.
