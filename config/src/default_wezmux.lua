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
-- Workspace switching uses Opt+Shift on macOS so we don't collide with
-- system screenshot shortcuts (Cmd+Shift+3/4/5) which are intercepted by
-- macOS before any app sees them.
local workspace_mod = is_windows and 'CTRL|ALT' or 'OPT|SHIFT'

-- Treat Option as a real Alt modifier so AZERTY composition doesn't swallow
-- the keypress before our binding fires.
config.send_composed_key_when_left_alt_is_pressed = false
config.send_composed_key_when_right_alt_is_pressed = false

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

return config
