# Next Step: Workspace Card Parity

## Why This Is Next

The workspace-first layout is now in place:

- left sidebar exists and switches workspaces
- tabs are scoped to the current workspace
- notifications and unread rings exist

The biggest remaining gap versus cmux is the sidebar itself. The cards still feel sparse and low-signal, so the next step should make each workspace preview immediately useful.

## Scope

Improve sidebar workspace cards so they show real, compact workspace context instead of placeholder text.

Focus on:

- current working directory preview that is readable at a glance
- git branch + dirty state formatting that is compact and consistent
- latest notification preview that truncates cleanly
- optional ports row when listening ports are present
- better visual hierarchy for active vs inactive workspaces

## Acceptance Criteria

- each workspace card shows a meaningful second line even with no notifications
- cards never render placeholder text like "No activity yet" unless there is truly no better signal
- long paths and notification text truncate cleanly without wrecking alignment
- active workspace is obvious without looking noisy
- sidebar looks closer to cmux in density and usefulness

## Out Of Scope

- CLI notification injection
- notification history panel
- drag-to-reorder workspaces
- Lua customization hooks

## Likely Files

- `wezterm-gui/src/termwindow/sidebar.rs`
- `wezterm-gui/src/termwindow/render/sidebar.rs`

## Follow-Up After This

After the cards are good enough, the next step should be navigation polish:

- `Cmd+Shift+U` jump to most recent unread pane
- "no unread notifications" status message when nothing is pending
