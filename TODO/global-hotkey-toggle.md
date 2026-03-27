# Global Hotkey Toggle (Quake-style)

**Priority**: High
**Status**: Not started
**Replaces**: Hammerspoon Alt+` workaround

## What

System-wide hotkey (default: Alt+`) that toggles Wezmux visibility from anywhere on the desktop, like iTerm2's hotkey window. Eliminates the need for Hammerspoon.

## Why

Currently requires Hammerspoon with a custom Lua config to get this behavior. Bundling it in the app makes setup zero-config and more reliable.

## Approach

Use Carbon `RegisterEventHotKey` API (same approach as iTerm2):
- No Accessibility permissions required (unlike CGEvent taps)
- Oldest and most reliable macOS global hotkey mechanism
- Small surface area (~80 lines in the macOS window backend)

### Implementation steps

1. **Carbon hotkey registration** in `window/src/os/macos/connection.rs`
   - Register hotkey on app launch via `RegisterEventHotKey`
   - Install Carbon event handler that fires on hotkey press
   - Unregister on app quit via `UnregisterEventHotKey`

2. **Toggle logic**
   - If any wezmux window is key/focused: hide all windows
   - If wezmux is not focused: show all windows + activate app
   - Use existing `window.show()` / `window.hide()` + `NSRunningApplication::activateWithOptions_`

3. **Config option**
   - Add `global_hotkey = "ALT|Backtick"` to config (wezterm.lua)
   - Parse into Carbon modifier mask + keycode
   - Default: Alt+` (matches current Hammerspoon binding)

## Files to touch

- `window/src/os/macos/connection.rs` — hotkey registration + event handler
- `config/src/lib.rs` or `config/src/window.rs` — config option
- `wezterm-gui/src/termwindow/mod.rs` — toggle handler (call existing show/hide)

## References

- iTerm2 hotkey window uses the same Carbon API
- `RegisterEventHotKey` docs: Carbon Event Manager
- Existing show/hide: `window/src/os/macos/window.rs` lines 1165-1207
