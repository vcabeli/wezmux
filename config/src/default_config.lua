local wezterm = require 'wezterm'
local config = wezterm.config_builder()

config.font = wezterm.font('Menlo')
config.font_size = 14

config.color_scheme = 'Monokai (terminal.sexy)'

config.window_background_opacity = 0.9
config.macos_window_background_blur = 20

config.hide_tab_bar_if_only_one_tab = true

-- Suppress native toasts from focused pane (sidebar shows them instead)
config.notification_handling = 'SuppressFromFocusedPane'
pcall(function() config.sidebar = { width = '400px' } end)

config.inactive_pane_hsb = {
  saturation = 0.5,
  brightness = 0.5,
}

local act = wezterm.action
config.keys = {
  -- Pane splitting (iTerm2: Cmd+D = side by side, Cmd+Shift+D = top/bottom)
  { key = 'd', mods = 'SUPER',       action = act.SplitHorizontal { domain = 'CurrentPaneDomain' } },
  { key = 'd', mods = 'SUPER|SHIFT', action = act.SplitVertical   { domain = 'CurrentPaneDomain' } },

  -- Pane navigation (iTerm2: Cmd+[ / Cmd+])
  { key = '[', mods = 'SUPER', action = act.ActivatePaneDirection 'Prev' },
  { key = ']', mods = 'SUPER', action = act.ActivatePaneDirection 'Next' },

  -- Close pane/tab (iTerm2: Cmd+W)
  { key = 'w', mods = 'SUPER', action = act.CloseCurrentPane { confirm = false } },

  -- Tabs (iTerm2: Cmd+T, Cmd+Left/Right)
  { key = 't',          mods = 'SUPER',       action = act.SpawnTab 'CurrentPaneDomain' },
  { key = 'LeftArrow',  mods = 'SUPER|SHIFT', action = act.ActivateTabRelative(-1) },
  { key = 'RightArrow', mods = 'SUPER|SHIFT', action = act.ActivateTabRelative(1) },

  -- Clear scrollback (iTerm2: Cmd+K)
  { key = 'k', mods = 'SUPER', action = act.ClearScrollback 'ScrollbackAndViewport' },

  -- Find (iTerm2: Cmd+F)
  { key = 'f', mods = 'SUPER', action = act.Search { CaseInSensitiveString = '' } },
}

return config
