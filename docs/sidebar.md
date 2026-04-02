# Sidebar

The sidebar is a persistent panel on the left side of the Wezmux window that shows one card per workspace. It gives you an at-a-glance view of what's happening across all your workspaces: which agent is running, what it's doing, git status, open PRs, listening ports, and unread notifications.

Toggle it with **Cmd+B**.

## What each card shows

From top to bottom, a workspace card displays:

| Element | Description |
|---------|-------------|
| **Title** | Workspace name (or custom display name), bold. Agent icon shown if an agent is detected. |
| **Unread badge** | Blue circle with count (1--9, or 9+) in the top-right corner. Only visible when there are unread notifications. |
| **Agent status** | Agent type icon + status indicator: Working (spinner), Idle, or Needs Input. See [Agent Integration](agent-integration.md). |
| **Preview** | Latest agent message (via [OSC 7777](osc7777.md)) or last notification. Shows up to 4 lines from the terminal buffer when no agent is running. |
| **Git branch** | Branch name with dirty indicator (`*`) if there are uncommitted changes. |
| **Git path** | Compact working directory path (e.g. `~/code/myproject`). |
| **Pull request** | PR number and status (Open, Merged, Closed) with color coding. Requires `gh` CLI. |
| **Listening ports** | Up to 3 ports shown (e.g. `:3000, :5173`), with a `+N` indicator if more are active. |

The active workspace card is highlighted with the accent color. Hovering a card shows a close button (x).

## Interactions

| Action | Effect |
|--------|--------|
| **Left-click** a card | Switch to that workspace |
| **Right-click** a card | Open context menu |
| **Scroll** on the sidebar | Scroll through workspace cards |
| **Drag** the right edge | Resize the sidebar width |

### Context menu

Right-clicking a workspace card opens a menu with:

- **Color** -- pick an accent color for the card (red, orange, yellow, green, cyan, blue, purple, pink, or reset to default)
- **Move Up / Down / to Top / to Bottom** -- reorder workspaces in the sidebar
- **Close Workspace** -- close all panes and tabs in the workspace

### Toolbar

At the bottom of the sidebar, two buttons:

- **Split Horizontal** / **Split Vertical** -- split the active pane

A fixed **New Workspace** button sits below the scrollable card area.

## Configuration

### Sidebar visibility and width

```lua
config.sidebar = {
  visible = true,      -- toggle with Cmd+B
  width = '400px',     -- accepts: 'Npx', 'Nxcells', '25%'
}
```

### Colors

All sidebar colors are customizable:

```lua
config.sidebar = {
  colors = {
    bg         = '#3a3a41',
    card_bg    = '#303036',
    card_hover = '#44444b',
    accent     = '#0091ff',
    text       = 'rgba(255,255,255,0.9)',
    muted      = 'rgba(255,255,255,0.55)',
    separator  = 'rgba(255,255,255,0.1)',
    pr_open    = '#b860ff',
    pr_merged  = '#4cc57c',
    pr_closed  = '#d76a6a',
  },
}
```

See the [color reference in the config docs](config.md#sidebar-color-reference) for what each key controls.

Colors accept any CSS-style value: hex (`#rrggbb`, `#rrggbbaa`), `rgb(r,g,b)`, `rgba(r,g,b,a)`, or named colors. Active card text is always white for contrast.

### Per-workspace customization

Each workspace can have a custom display name and accent color. These are set via the context menu (right-click a card) and persisted in `~/.config/wezmux/workspaces.json`. Workspace ordering is also saved there.

## Metadata polling

The sidebar polls workspace metadata in the background so it never blocks rendering:

- **Git branch and dirty status** -- read from `.git/HEAD` in the workspace's working directory
- **Pull request status** -- fetched via `gh pr list` (degrades gracefully if `gh` is not installed)
- **Listening ports** -- scanned from the network stack for processes in the workspace

Polling is coalesced with a 200ms delay to avoid thrashing when multiple workspaces change at once. Results are cached and survive session restores.
