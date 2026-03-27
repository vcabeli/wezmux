# Fix: Preview not updating when workspace is focused

**Priority**: High
**Status**: Not started

## What

The Claude Code output preview on sidebar cards stops updating (or updates inconsistently) when the workspace is focused and no new notifications are firing. The preview should always reflect recent terminal output regardless of focus or notification state.

## Symptoms

- Preview text goes stale while actively watching Claude Code work in the focused pane
- Switching away and back sometimes refreshes it
- Likely tied to the preview cache invalidation only triggering on notification count changes

## Investigation areas

- Preview cache refresh logic — currently 200ms interval, but may be gated on notification/structural changes rather than a pure timer
- Sidebar entry cache — invalidation condition may skip redraws when nothing "changed" from the sidebar's perspective, even though terminal buffer content did
- The focused workspace's card may be deprioritized since the user can see the terminal directly
