# Workspace Card Context Menu

**Priority**: Medium
**Status**: Done (except Rename)

## Done

- Right-click on sidebar workspace card opens native macOS context menu
- **Set color** — submenu with 8 preset colors + reset (persisted per workspace via `~/.config/wezmux/workspaces.json`)
- **Move up / down / to top / to bottom** — reorder cards in sidebar
- **Close workspace** — close all panes, clean up persisted config
- Accent color bar shown on cards with custom color (active and inactive)
- Native `NSMenu` integration with `popUpContextMenu` on macOS
- Context menu selection routed via `ContextMenuNotification` through window event system

## Still TODO

- **Rename workspace** — inline edit or modal to change display name

## Files

- `window/src/lib.rs` — `ContextMenuItem`, `ContextMenuNotification`, `WindowOps::show_context_menu`
- `window/src/os/macos/window.rs` — native NSMenu implementation
- `wezterm-gui/src/termwindow/mouseevent.rs` — `show_workspace_context_menu`, `close_workspace`, `handle_context_menu_selection`
- `wezterm-gui/src/termwindow/sidebar.rs` — `context_menu_workspace`, `workspace_configs`, `ordered_workspaces`
- `wezterm-gui/src/termwindow/workspace_config.rs` — persistence layer for workspace customizations
- `wezterm-gui/src/termwindow/render/sidebar.rs` — accent color rendering on cards
