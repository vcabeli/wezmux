# Workspace Card Context Menu

**Priority**: Medium
**Status**: Not started

## What

Right-click on a sidebar workspace card opens a context menu with personalization and management options:

- **Rename workspace** — inline edit or modal to change display name
- **Set color** — pick a color for the card's left accent bar (persisted per workspace)
- **Move up** / **Move down** — reorder card in sidebar
- **Move to top** / **Move to bottom** — jump to extremes
- **Close workspace** — close all panes in workspace (same as existing × button)

## Why

Workspaces are currently anonymous and fixed-order. When running 5-10+ agent workspaces, color-coding and custom names make it much faster to visually identify which workspace is which. Reordering lets you group related workspaces together.

## Approach

### Context menu rendering

- Build menu as a `box_model::Element` tree (same pattern as sidebar cards)
- Position at cursor location on right-click
- Dismiss on click-outside or Escape
- Add `UIItemType::ContextMenu(ContextMenuItem)` for hit-testing

### Menu items

| Item | Action |
|------|--------|
| Rename | Show inline text input on the card title, confirm on Enter |
| Set color | Show color submenu with ~8 preset colors (red, orange, yellow, green, cyan, blue, purple, pink) |
| Move up | Swap with previous card in sidebar order |
| Move down | Swap with next card in sidebar order |
| Move to top | Move card to index 0 |
| Move to bottom | Move card to last index |
| Close workspace | Kill all panes, remove workspace from mux |

### Persistence

- Store workspace customizations (name override, color, order) in a JSON file (`~/.config/wezmux/workspaces.json` or similar)
- Key by original workspace name
- Load on startup, save on change

### Card accent bar color

- Currently the active card has a left accent bar
- Extend to all cards: user-chosen color for accent bar, fallback to default (blue for active, none for inactive)
- Color stored as hex string in workspace config

## Related

- [workspace-drag-reorder.md](workspace-drag-reorder.md) — drag-to-reorder provides the same reordering via direct manipulation; shares the workspace order persistence layer

## Files to touch

- `wezterm-gui/src/termwindow/render/sidebar.rs` — right-click handler, menu rendering, accent bar color
- `wezterm-gui/src/termwindow/mouseevent.rs` — new `UIItemType` variant for context menu items
- `wezterm-gui/src/termwindow/box_model.rs` — menu element construction (reuse existing patterns)
- New: workspace config persistence module (load/save JSON)
