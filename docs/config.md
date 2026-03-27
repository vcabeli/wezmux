# Wezmux Configuration

Wezmux uses the same Lua config system as WezTerm. Config files are loaded in priority order:

1. `WEZMUX_CONFIG_FILE` env var (if set)
2. `WEZTERM_CONFIG_FILE` env var (if set)
3. `~/.wezmux.lua`
4. `~/.wezterm.lua`
5. `~/.config/wezterm/wezmux.lua`
6. `~/.config/wezterm/wezterm.lua`

If no config file is found, a built-in default config is used.

Config files hot-reload on save.

## Minimal config

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.font = wezterm.font('Menlo')
config.font_size = 14
config.color_scheme = 'Monokai (terminal.sexy)'

return config
```

## Sidebar

```lua
config.sidebar = {
  visible = true,
  width = '400px',
  colors = {
    bg       = '#3a3a41',
    card_bg  = '#303036',
    card_hover = '#44444b',
    accent   = '#0091ff',
    text     = 'rgba(255,255,255,0.9)',
    muted    = 'rgba(255,255,255,0.55)',
    separator = 'rgba(255,255,255,0.1)',
    pr_open   = '#b860ff',
    pr_merged = '#4cc57c',
    pr_closed = '#d76a6a',
  },
}
```

### Sidebar color reference

| Key | Default | Used for |
|-----|---------|----------|
| `bg` | `#3a3a41` | Sidebar background |
| `card_bg` | `#303036` | Inactive card background |
| `card_hover` | `#44444b` | Card background on hover |
| `accent` | `#0091ff` | Active card bg, accent bar, unread badge, "needs input" indicator |
| `text` | `rgba(255,255,255,0.9)` | Primary text on inactive cards |
| `muted` | `rgba(255,255,255,0.55)` | Secondary/preview text on inactive cards |
| `separator` | `rgba(255,255,255,0.1)` | Divider lines between sections |
| `pr_open` | `#b860ff` | Open PR status (purple) |
| `pr_merged` | `#4cc57c` | Merged PR status (green) |
| `pr_closed` | `#d76a6a` | Closed PR status (red) |

Colors accept any CSS-style value: hex (`#rrggbb`, `#rrggbbaa`), `rgb(r,g,b)`, `rgba(r,g,b,a)`, or named colors.

Active card text is always white — it's derived automatically to ensure contrast against the accent color.

## Keyboard shortcuts (default)

| Key | Action |
|-----|--------|
| `Cmd+D` | Split pane horizontally |
| `Cmd+Shift+D` | Split pane vertically |
| `Cmd+[` / `Cmd+]` | Navigate panes |
| `Cmd+W` | Close pane |
| `Cmd+T` | New tab |
| `Cmd+Shift+Left/Right` | Switch tabs |
| `Cmd+K` | Clear scrollback |
| `Cmd+F` | Find |
| `Alt+`` ` | Toggle Wezmux visibility (global hotkey) |

## Other settings

All standard [WezTerm configuration](https://wezfurlong.org/wezterm/config/files.html) works in `~/.wezmux.lua`.
