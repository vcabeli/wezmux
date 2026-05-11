local wezterm = require 'wezterm'
local config = wezterm.config_builder()
local target_triple = wezterm.target_triple or ''
local is_windows = target_triple:find('windows') ~= nil

if is_windows then
  config.font = wezterm.font('JetBrains Mono')
else
  config.font = wezterm.font('Menlo')
  config.macos_window_background_blur = 20
end
config.font_size = 14

config.color_scheme = 'Monokai (terminal.sexy)'

config.window_background_opacity = 0.9

config.hide_tab_bar_if_only_one_tab = true

-- Side bar --
pcall(function()
  config.sidebar = {
    width = '600px',
    colors = {
      bg = '#515161',
      accent = '#5091ff',
    },
  }
end)

-- Suppress native toasts from focused pane (sidebar shows them instead)
config.notification_handling = 'SuppressFromFocusedPane'

config.inactive_pane_hsb = {
  saturation = 0.5,
  brightness = 0.5,
}

local act = wezterm.action
local primary_mod = is_windows and 'CTRL|SHIFT' or 'SUPER'
local secondary_mod = is_windows and 'CTRL|ALT' or 'SUPER|SHIFT'
-- Workspace switching uses Cmd+Ctrl on macOS. Cmd+Shift+3/4/5 are
-- screenshot shortcuts that macOS intercepts, and Opt+Shift+<digit>
-- produces bracket characters on AZERTY (Opt+Shift+5 = `[`,
-- Opt+Shift+°` = `]`), which breaks tmux prefix sequences like `C-b [`
-- for scrollback. Cmd+Ctrl avoids both: no system shortcut on digits,
-- and Cmd suppresses Option-key composition so AZERTY users get the
-- raw key event. phys:N (below) still targets the hardware key
-- regardless of layout.
local workspace_mod = is_windows and 'CTRL|ALT' or 'SUPER|CTRL'

config.keys = {
  -- Pane splitting
  { key = 'd', mods = primary_mod,   action = act.SplitHorizontal { domain = 'CurrentPaneDomain' } },
  { key = 'd', mods = secondary_mod, action = act.SplitVertical   { domain = 'CurrentPaneDomain' } },

  -- Pane navigation
  { key = '[', mods = primary_mod, action = act.ActivatePaneDirection 'Prev' },
  { key = ']', mods = primary_mod, action = act.ActivatePaneDirection 'Next' },

  -- Close pane/tab
  { key = 'w', mods = primary_mod, action = act.CloseCurrentPane { confirm = false } },

  -- Tabs
  { key = 't',          mods = primary_mod,   action = act.SpawnTab 'CurrentPaneDomain' },
  { key = 'LeftArrow',  mods = secondary_mod, action = act.ActivateTabRelative(-1) },
  { key = 'RightArrow', mods = secondary_mod, action = act.ActivateTabRelative(1) },

  -- Clear scrollback
  { key = 'k', mods = primary_mod, action = act.ClearScrollback 'ScrollbackAndViewport' },

  -- Find
  { key = 'f', mods = primary_mod, action = act.Search { CaseInSensitiveString = '' } },

  -- Reload config (helps when debugging keybindings)
  { key = 'r', mods = secondary_mod, action = act.ReloadConfiguration },

  -- Workspace switching. phys:N targets the physical digit-row position so
  -- AZERTY (where digits live behind Shift) doesn't break. SwitchToWorkspace
  -- auto-creates the workspace if it doesn't exist yet.
  { key = 'phys:1', mods = workspace_mod, action = act.SwitchToWorkspace { name = '1' } },
  { key = 'phys:2', mods = workspace_mod, action = act.SwitchToWorkspace { name = '2' } },
  { key = 'phys:3', mods = workspace_mod, action = act.SwitchToWorkspace { name = '3' } },
  { key = 'phys:4', mods = workspace_mod, action = act.SwitchToWorkspace { name = '4' } },
  { key = 'phys:5', mods = workspace_mod, action = act.SwitchToWorkspace { name = '5' } },
  { key = 'phys:6', mods = workspace_mod, action = act.SwitchToWorkspace { name = '6' } },
  { key = 'phys:7', mods = workspace_mod, action = act.SwitchToWorkspace { name = '7' } },
  { key = 'phys:8', mods = workspace_mod, action = act.SwitchToWorkspace { name = '8' } },
  { key = 'phys:9', mods = workspace_mod, action = act.SwitchToWorkspace { name = '9' } },

  -- Workspace launcher (fuzzy list of all workspaces)
  { key = 'phys:0', mods = workspace_mod, action = act.ShowLauncherArgs { flags = 'FUZZY|WORKSPACES' } },
}

-- Cmd+click (Ctrl+click on Windows) to open links. The default plain-click
-- binding only fires when mouse reporting is off, so it doesn't work inside
-- TUI apps like vim/tmux/lazygit. Modifier-clicks bypass mouse reporting,
-- so this works everywhere — matching the macOS convention used by Terminal,
-- iTerm2, and VS Code.
local link_mod = is_windows and 'CTRL' or 'SUPER'
config.mouse_bindings = {
  {
    event = { Up = { streak = 1, button = 'Left' } },
    mods = link_mod,
    action = act.OpenLinkAtMouseCursor,
  },
}

return config
