# Sidebar config via ~/.wezmux.lua

**Priority**: Medium
**Status**: Not started

## What

Expose sidebar customization through a dedicated `~/.wezmux.lua` config file, separate from `wezterm.lua`. This keeps wezmux-specific settings isolated and avoids polluting the main WezTerm config.

## Why

- Wezmux features (sidebar, notifications, workspace colors) are orthogonal to WezTerm terminal config
- Users may want to share their `wezterm.lua` without wezmux-specific clutter
- Cleaner upgrade path — wezmux config won't conflict with upstream WezTerm changes

## What to expose

- Sidebar width (default + min/max)
- Sidebar position (left / right)
- Sidebar visibility on startup
- Poll intervals (git, PR, ports, preview refresh)
- Default workspace accent colors
- Card layout options (show/hide preview, show/hide PR status, show/hide ports)
- Color scheme (sidebar bg, card bg, active card bg, text colors, accent bar color, unread badge color, notification ring color)
- Global hotkey binding (see [global-hotkey-toggle.md](global-hotkey-toggle.md))
- Notification behavior (desktop notifications on/off, suppression rules)

## Current state

Some sidebar options are already in the config system via `wezterm.lua` (see `config/src/config.rs` `Sidebar` struct):
- `config.sidebar = { width = '280px', visible = true }`

Colors are **not** configurable — they're hardcoded as functions in `sidebar.rs` (`sidebar_bg()`, `sidebar_card_bg()`, `sidebar_card_hover()`, etc.). These need to be extracted into the config struct first.

## Approach

- Load `~/.wezmux.lua` at startup, after `wezterm.lua`
- Wezmux-specific Lua API namespace (e.g., `wezmux.config {}`)
- Fall back to sensible defaults if file doesn't exist
- Hot-reload on file change (same mechanism WezTerm uses for `wezterm.lua`)

## Files to touch

- New: config loader for `~/.wezmux.lua`
- `wezterm-gui/src/termwindow/render/sidebar.rs` — read values from wezmux config
- `config/src/` — new wezmux config struct alongside existing WezTerm config
