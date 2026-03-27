# Sidebar config via ~/.wezmux.lua

**Priority**: Medium
**Status**: Partial — config loading + sidebar colors done, more options TODO

## Done

- `~/.wezmux.lua` loaded as highest-priority config (before `~/.wezterm.lua`)
- `WEZMUX_CONFIG_FILE` env var override supported
- Built-in default config embedded in binary (zero-config experience)
- Sidebar colors fully configurable via `config.sidebar.colors`
- Hot-reload works (same watcher mechanism as wezterm.lua)

## Still TODO

- Sidebar position (left / right)
- Poll intervals (git, PR, ports, preview refresh)
- Default workspace accent colors (per-workspace)
- Card layout options (show/hide preview, show/hide PR status, show/hide ports)
- Notification behavior (desktop notifications on/off, suppression rules)
