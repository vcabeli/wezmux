# Workspace Drag-to-Reorder

**Priority**: Medium
**Status**: Not started

## What

Left-click and drag a workspace card in the sidebar to reorder it. Visual feedback during drag (ghost card or insertion indicator). Drop to confirm new position.

## Why

Complements the right-click context menu reordering (move up/down/top/bottom) with a more intuitive direct manipulation option. When managing many workspaces, drag-and-drop is the fastest way to organize them.

## Approach

### Drag detection

- On left mouse down on a sidebar card, start a drag after ~5px movement threshold (avoid triggering on normal clicks)
- Track drag state: `dragging: Option<(workspace_name, cursor_y, original_index)>`

### Visual feedback during drag

- Draw a semi-transparent ghost of the dragged card at cursor position
- Show an insertion line (horizontal bar) between cards at the drop target
- Dim the original card in its source position

### Drop logic

- On mouse up: compute target index from cursor Y position relative to card boundaries
- Update workspace order in the sidebar order list
- Persist new order (same storage as context menu reordering — `workspaces.json`)

### Edge cases

- Drag beyond sidebar top/bottom: clamp to first/last position
- Drag outside sidebar horizontally: cancel drag, snap back
- Single click (no movement): normal workspace switch behavior preserved

## Related

- [workspace-card-context-menu.md](workspace-card-context-menu.md) — context menu provides the same reordering via move up/down/top/bottom; shares the workspace order persistence layer

## Files to touch

- `wezterm-gui/src/termwindow/render/sidebar.rs` — drag state, ghost rendering, insertion indicator
- `wezterm-gui/src/termwindow/mouseevent.rs` — drag detection, mouse move/up handlers
- Shared workspace order persistence with context menu TODO
