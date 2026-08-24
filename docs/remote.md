# Remote Sessions

A workspace can run on another machine. Click **+ New workspace** in the [sidebar](sidebar.md) and pick where it runs: this machine, or one of your remote hosts.

Remote workspaces are persistent. Close the window, lose the network, quit the app — the workspaces keep running on that host, and the picker offers them back under their old names. One window can hold local and remote workspaces side by side; the [sidebar](sidebar.md) shows each one's own git branch, PR, ports and agent status.

## How it works

Wezmux connects over ssh to a *wezmux mux server* on the remote host and speaks the mux protocol to it. The mux server owns the panes, so nothing dies when the connection does.

```
 your Mac                                build-box
┌───────────────────────┐               ┌─────────────────────────────┐
│ wezterm-gui           │               │ wezterm-mux-server          │
│  ├─ window + sidebar  │  ssh + mux    │  ├─ workspace "api"         │
│  └─ client domain ────┼──protocol────►│  │   └─ claude, npm run dev │
│                       │               │  └─ workspace "web"         │
└───────────────────────┘               └─────────────────────────────┘
```

Two things flow back to the sidebar over that connection:

- **Agent status.** The remote hook scripts emit [OSC 7777](osc7777.md) into the pane; the remote mux server parses it and forwards it to the GUI as an alert. Notification badges ([OSC 9](notifications.md)) arrive the same way.
- **Workspace metadata.** The GUI asks the server for each workspace's git branch/dirty state, pull request and listening ports. The server is the side that knows the panes' real working directories and process ids, so `git`, `gh` and `lsof` all run *there*.

Because both halves are wezmux-specific, the remote host needs **wezmux**, not stock wezterm. The mux protocol version is checked on connect, so a mismatch fails with a clear message rather than silently dropping features.

## Installing wezmux on the remote host

```bash
bin/wezmux-install-remote --install-deps --install-rust build-box
# or, equivalently for a host that is already set up:
make install-remote HOST=build-box
```

This copies your local checkout to the host, builds it there, and installs into `~/.local/lib/wezmux/bin`:

| File | Purpose |
|------|---------|
| `wezterm` | CLI that starts and proxies to the mux server |
| `wezterm-mux-server` | owns the remote panes |
| `claude` | wrapper that injects the agent status hooks |
| `hooks/` | the hook scripts themselves |

The wrapper and hooks live in the same directory as the binaries on purpose: the mux server finds them by looking for a sibling `bin/claude`, sets `WEZMUX=1` and `WEZMUX_BIN`, and puts them on the PATH of every shell it spawns. That is what makes `claude` on the remote host pick up wezmux's hooks automatically, exactly as it does locally.

Two ways to get it there:

| | On the host | Time |
|---|---|---|
| `--binaries DIR` | 56 MB installed, nothing else | the upload |
| build there (default) | + ~115 MB of sources and ~1.3 GB of build output, plus ~1.9 GB for a toolchain if `--install-rust` puts one there | a few minutes |

`--binaries DIR` installs binaries you already have for that platform — from CI, another machine of the same kind, or a cross build — and needs no toolchain, no build dependencies and no sources on the host. Otherwise the host needs `rsync`, `cmake`, `pkg-config`, a C/C++ compiler, the OpenSSL headers and a Rust toolchain; it does **not** need the GUI dependencies from `./get-deps`, since only the CLI and the mux server are built there.

| Option | Effect |
|--------|--------|
| `--binaries DIR` | install prebuilt binaries instead of building on the host |
| `--install-deps` | install the missing OS packages (apt/dnf/pacman/zypper) |
| `--install-rust` | install a Rust toolchain with rustup (no sudo needed) |
| `--data-dir DIR` | put sources, build output, toolchain and the install under `DIR` |
| `--prefix DIR` | install location (default `~/.local/lib/wezmux`) |
| `--src DIR` | where to copy the sources (default `~/.cache/wezmux/src`) |
| `--target-dir DIR` | cargo target directory (default `SRC/target`) |
| `--cargo-home DIR`, `--rustup-home DIR` | toolchain locations |
| `--no-build` | copy the sources and stop |
| `--no-link` | don't symlink the binaries into `~/.local/bin` |
| `--jobs N` | passed to cargo as `--jobs N` |

`--install-deps` is the only part that needs root. It uses the host's own sudo: passwordless where configured, otherwise it prompts on a terminal, and if neither is possible it prints the command for you to run. Nothing is installed with sudo unless you pass that flag.

The build needs a few GB. If the home filesystem is too small, put everything on a bigger one:

```bash
bin/wezmux-install-remote --data-dir /scratch/wezmux build-box
```

The script checks free space before starting a build that cannot finish, and tells you which flag to use.

**Keep both sides on the same version.** The mux protocol version is checked on connect, so re-run the install script after updating your local checkout. A mux server that was already running keeps serving the old build; the script warns you when it sees one, and restarting it means losing the panes it hosts:

```bash
ssh build-box "pkill -f '[w]ezterm-mux-server'"
```

## Where wezmux looks for the remote CLI

On connect, wezmux probes the host for `~/.local/lib/wezmux/bin/wezterm` — where the install script puts it — and then for `wezterm` on the PATH. A host installed with the script therefore needs no configuration at all.

For an install somewhere else, name it once in `~/.wezmux.lua`:

```lua
config.ssh_domains = {
  {
    name = 'SSHMUX:build-box',
    remote_address = 'build-box',
    multiplexing = 'WezTerm',
    remote_wezterm_path = '/opt/wezmux/bin/wezterm',
  },
}
```

Hosts listed there are also offered by the picker before you have ever used them, and appear in the launcher menu (right-click the tab bar's `+` button, or bind `ShowLauncher`).

## Putting `wezmux` on your PATH

`make install` builds the launcher into the app bundle; symlink it once:

```bash
mkdir -p ~/.local/bin
ln -sf /Applications/Wezmux.app/Contents/Resources/bin/wezmux ~/.local/bin/wezmux
# or system-wide (needs sudo):
# sudo ln -sf /Applications/Wezmux.app/Contents/Resources/bin/wezmux /usr/local/bin/wezmux
```

The launcher runs `wezterm-gui` directly (never the `wezterm` front end, which may hand off to an already running instance — not what you want when the point is to start a remote session). It prefers a release build over a debug build.

## The picker

**+ New workspace** lists, in order:

| Row | What it does |
|-----|--------------|
| `Local` | a new workspace on this machine, as before |
| `<workspace> · <host>` | reopen a workspace used before; marked `running` when it is already attached |
| `New workspace on <host>` | a new workspace on a host you have used, or one named in your config |
| `Other host…` | prompts for `[user@]host[:port]` |

Type to filter, `↑`/`↓` to move, `enter` to open, `esc` to cancel. With no remote host known, there is nothing to choose, so the button spawns a local workspace directly.

Hosts used before are remembered in `~/.config/wezmux/remote-sessions.json`, most recent first.

For a host where nothing can be installed, `wezmux ssh HOST` still opens a plain ssh window: no persistence, and the sidebar cannot report that host's git/PR/port state, because there is no server on the far side to ask.

## Troubleshooting

**"wezmux was not found on HOST".** Either it isn't installed there (`bin/wezmux-install-remote HOST`), or it is somewhere the probe doesn't look. Set `remote_wezterm_path` for the host, as [above](#where-wezmux-looks-for-the-remote-cli).

**"Please install the same version of wezmux on both the client and server".** Re-run the install script after updating locally, and restart the remote mux server if one was already running — it keeps serving the old build. This message also appears when the host runs stock wezterm rather than wezmux.

**Sidebar shows the workspace but no branch/PR/ports.** The server could not resolve a working directory for the workspace, which usually means the remote shell isn't reporting its cwd. Install wezterm's [shell integration](shell-integration.md) on the remote host so the shell emits OSC 7.

**No agent status for remote agents.** Check that `claude` on the remote host resolves to the wrapper: `ssh HOST 'echo $WEZMUX_BIN'` inside a wezmux pane should print the install directory, and `which claude` should point into it.

**Verbose logging.** Start the GUI with `RUST_LOG=wezterm_client=debug,mux=debug` to see the connection and metadata traffic.

## Why not tmux

tmux would give persistence with nothing to install, and wezterm can drive it in control mode. Two things make it a poor fit here:

- tmux discards OSC sequences it does not recognise, and the agent hooks report status with [OSC 7777](osc7777.md). It would take `allow-passthrough` on the server plus escape wrapping in every hook to get agent status through.
- There would be no server to answer the sidebar's metadata request, so the git, PR and port lines would need a second ssh channel per refresh.

Both of the things the sidebar exists to show would regress, so wezmux runs its own mux server instead and keeps the install as small as it can.

## Limitations

- The remote host must be able to run the wezmux mux server (Linux and macOS; the *GUI* remains macOS only).
- Local session restore is skipped for remote sessions on purpose: the remote mux is the source of truth for what is running.
- A workspace whose panes are split across local and remote domains reports metadata from whichever domain owns its active pane.
