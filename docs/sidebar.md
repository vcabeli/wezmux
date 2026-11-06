# Sidebar

The sidebar is a persistent panel on the left side of the Wezmux window that shows one card per workspace. It gives you an at-a-glance view of what's happening across all your workspaces: which agent is running, what it's doing, git status, open PRs, listening ports, and unread notifications.

Toggle it with **Cmd+B**.

## What each card shows

From top to bottom, a workspace card displays:

| Element | Description |
|---------|-------------|
| **Title** | Workspace name (or custom display name), bold. Agent icon shown if an agent is detected. |
| **Unread badge** | Outlined count (1--9, or 9+) before the title. Only visible when there are unread notifications. |
| **Agent status** | Agent type icon + status indicator: Working (spinner), Idle, or Needs Input. See [Agent Integration](agent-integration.md). |
| **Preview** | Latest agent message (via [OSC 7777](osc7777.md)) or last notification. Shows up to 4 lines from the terminal buffer when no agent is running. |
| **Git branch** | Branch name with dirty indicator (`*`) if there are uncommitted changes. |
| **Git path** | Compact working directory path (e.g. `~/code/myproject`). |
| **Pull request** | PR number and status (Open, Merged, Closed) with color coding. Requires `gh` CLI. |
| **Listening ports** | Up to 3 ports shown (e.g. `:3000, :5173`), with a `+N` indicator if more are active. |

Cards are drawn with single-pixel lines: no fill at rest, a light fill on hover, and a 1px outline in the accent color (or the workspace's own color) around the active one.

## Groups

Workspaces can be gathered into named groups, by theme or by repository (for example, the worktrees of one project). Grouping is manual: nothing is grouped until you ask.

A group shows as a header (a color swatch, its name and how many workspaces it holds) with its workspaces hanging from a thin rail in the group's color. Click the header to collapse or expand the group. A collapsed group still shows the workspace you are in, and lists on its header how many of its agents need input or are working. The **+** on the header creates a new workspace in that group.

A group sits where its first workspace would be in the sidebar order, and its workspaces stay together. Moving a workspace up or down moves it within its group; a workspace outside any group steps over a whole group at once. A group disappears when its last workspace closes or leaves it.

### Shortcuts

| Keys | Action |
|------|--------|
| **Cmd+Shift+N** | New workspace, outside any group |
| **Opt+Cmd+N** | New workspace in the current workspace's group |
| **Cmd+Shift+G** | Move the current workspace to a group: a menu of groups, plus **New Group…** and **Remove from Group** |
| **Opt+Cmd+G** | Take the current workspace out of its group |
| **Opt+Cmd+R** | Rename the current workspace's group |
| **Opt+Cmd+E** | Collapse or expand the current workspace's group |

The group shortcuts match physical keys, so they work on any keyboard layout even though Option is held. They are also in the **Shell › Workspace Groups** menu and the command palette.

## Interactions

| Action | Effect |
|--------|--------|
| **Left-click** a card | Switch to that workspace |
| **Right-click** a card | Open the workspace menu |
| **Left-click** a group header | Collapse or expand the group |
| **Right-click** a group header | Open the group menu |
| **Scroll** on the sidebar | Scroll through workspace cards |
| **Drag** the right edge | Resize the sidebar width |

### Context menus

Right-clicking a workspace card opens a menu with:

- **Rename…** -- set a display name for the workspace
- **Workspace Color** -- pick an accent color for the card (red, orange, yellow, green, cyan, blue, purple, pink, or reset to default)
- **Workspace Emoji…** -- pick an emoji shown after the title
- **Move to Group** -- the existing groups (the current one is ticked), and **New Group…**, which asks for a name
- **Remove from Group** -- shown when the workspace is in one
- **Move Up / Down / to Top / to Bottom** -- reorder workspaces (within their group, if they have one)
- **Close Workspace** -- close all panes and tabs in the workspace

Right-clicking a group header opens a menu with:

- **Rename Group…**
- **Collapse** / **Expand**
- **New Workspace in Group**
- **Group Color** -- the same swatches, used for the header and the rail
- **Move Group Up / Down**
- **Ungroup** -- delete the group, leaving its workspaces where they are

### Toolbar

At the bottom of the sidebar, two buttons:

- **Split Horizontal** / **Split Vertical** -- split the active pane

A fixed **New Workspace** button (Cmd+Shift+N) sits below the scrollable card area.

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

Colors accept any CSS-style value: hex (`#rrggbb`, `#rrggbbaa`), `rgb(r,g,b)`, `rgba(r,g,b,a)`, or named colors. The active card's title is always white. `card_bg` is no longer used: cards take the sidebar's `bg`.

### Per-workspace customization

Each workspace can have a custom display name and accent color. These are set via the context menu (right-click a card) and persisted in `~/.config/wezmux/workspaces.json`. Workspace ordering, groups (name, color, collapsed state) and each workspace's group are also saved there.

## Metadata polling

The sidebar polls workspace metadata in the background so it never blocks rendering:

- **Git branch** -- read from `.git/HEAD` in the workspace's working directory on every refresh
- **Git dirty status** -- a `git status` scan of the working tree, run at most every 5 seconds per workspace. A scan that takes longer is repeated less often in proportion to its cost, so a huge tree with a large untracked footprint costs a small fraction of a core rather than a whole one. Switching branch or repository forces a fresh scan, and workspaces open on the same repository share a single scan.
- **Pull request status** -- fetched via `gh pr list` (degrades gracefully if `gh` is not installed)
- **Listening ports** -- scanned from the network stack for processes in the workspace, at most every 5 seconds per workspace unless its processes change

Polling is coalesced with a 200ms delay to avoid thrashing when multiple workspaces change at once. Results are cached and survive session restores.
