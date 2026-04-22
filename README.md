<p align="center">
  <img src="assets/wezmux-logo.png" alt="Wezmux logo" width="200">
</p>

# Wezmux

A fork of [WezTerm](https://github.com/wezterm/wezterm) that adds workspace management for multi-agent terminal workflows. Wezmux focuses on running multiple coding agents side-by-side and making each workspace's state visible at a glance.

![Wezmux screenshot](assets/screenshot.png)

## Why ?

Because cmux was disappointing (and I don't need a browser in my terminal) and I mean WezTerm is just the dream

## What's different from WezTerm

**Persistent sidebar** showing per-workspace metadata:
- Git branch and dirty status
- PR number and status (via `gh` CLI)
- Listening ports
- Agent status line (via OSC 7777)
- Unread notification badges

**Notification tracking** (on top of WezTerm's existing OSC 9 toasts):
- Blue ring on panes with unread notifications
- Unread badge counts in the sidebar
- `Option+U` to jump to the last unread notification

**Agent integrations:**
- **Claude Code** — hooks injected automatically via wrapper script (no setup needed)
- **Codex** — run `make install-codex-hooks` to set up hooks in `~/.codex/hooks.json`
- Status, tool activity, and output previews shown in the sidebar for both

**OSC 7777 agent status protocol** for structured status reporting:
```
\e]7777;status;working;Running tests\a
```

**Session save/restore** on quit and relaunch:
- Workspace layout and split pane structure preserved
- Per-pane CWDs restored
- Scrollback history with ANSI colors
- Sidebar metadata cache (no blank-state flash on relaunch)

**Keyboard shortcuts:**
| Shortcut | Action |
|----------|--------|
| ``Option+ ` `` | Show/hide Wezmux from anywhere |
| `Cmd+B` | Toggle sidebar |
| `Option+K/J` | Switch to previous/next workspace |
| `Option+1..9` | Switch to workspace by index |
| `Option+U` | Jump to last unread notification |

## Install

Public Wezmux `v1.0` support is currently macOS-only. The source tree still
contains inherited upstream cross-platform code, but builds outside the
documented macOS path should be treated as best-effort until Wezmux-specific
support is published for them.

### Prerequisites

- **Rust toolchain** — installed automatically via `rust-toolchain.toml` once rustup is present. Install via Homebrew:
  ```bash
  brew install rust
  ```
  Or via rustup:
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **Xcode Command Line Tools** — needed for C dependencies (harfbuzz, freetype, libpng, zlib):
  ```bash
  xcode-select --install
  ```
- **`gh` CLI** (optional) — enables PR status in the sidebar:
  ```bash
  brew install gh
  ```

### Clone

```bash
git clone --recursive https://github.com/vcabeli/wezmux.git
cd wezmux
```

If you already cloned without `--recursive`, run:
```bash
git submodule update --init --recursive
```

### Build and install

```bash
make install
```

This builds release binaries, assembles `Wezmux.app`, ad-hoc codesigns the main binary, and installs to `/Applications/Wezmux.app`. Claude Code hooks are injected automatically via the bundled wrapper script.

To set up Codex integration (merges hooks into `~/.codex/hooks.json`):
```bash
make install-codex-hooks
```

To install to a custom location:
```bash
APP_DIR=~/Applications/Wezmux.app make install
```

### Development build

Build to `target/Wezmux.app` without touching `/Applications`:

```bash
make bundle
open target/Wezmux.app
```

## Config

Wezmux prefers `~/.wezmux.lua`, but it will also read `~/.wezterm.lua` for compatibility with existing WezTerm setups.

If you share config with stock WezTerm, guard Wezmux-specific fields so upstream WezTerm doesn't error:

```lua
pcall(function()
  config.sidebar = { width = '400px' }
end)
```

## Documentation

- [Sidebar](docs/sidebar.md) — what each card shows, interactions, and configuration
- [Agent Integration](docs/agent-integration.md) — Claude Code, Codex, and custom agent hooks
- [OSC 7777 Protocol](docs/osc7777.md) — structured agent status escape sequence reference
- [Notifications](docs/notifications.md) — notification lifecycle and visual indicators
- [Configuration](docs/config.md) — Lua config reference (sidebar colors, keyboard shortcuts)

## Credits

Built on top of [WezTerm](https://github.com/wezterm/wezterm) by [@wez](https://github.com/wez). All the heavy lifting (GPU renderer, terminal emulation, multiplexer) is WezTerm's.

With Claude Code and Opus 4.6, a little bit of planning with [GSD](https://github.com/gsd-build/get-shit-done) and occasional Codex with GPT 5.4.
