//! Graceful shutdown of AI-agent processes (claude, codex, omp, cursor,
//! opencode, aider) running inside panes.
//!
//! Sending SIGTERM lets the agent print its "Resume this session with: ..."
//! line, which is then captured by the saved scrollback so the conversation
//! ID is visible after Wezmux restarts.

use crate::pane::{CachePolicy, Pane};
use crate::window::WindowId;
use crate::Mux;
use procinfo::LocalProcessInfo;
use std::sync::Arc;
use std::time::Duration;

/// Pause after sending SIGTERM so each agent has time to write its goodbye
/// message into the PTY before we snapshot the scrollback.
const GRACE_PERIOD: Duration = Duration::from_millis(800);

fn is_agent_exe_name(name: &str) -> bool {
    // Mirror of detect_agent_type() in wezterm-gui/src/termwindow/sidebar.rs.
    let lower = name.to_lowercase();
    lower.contains("claude")
        || lower == "codex"
        || lower == "omp"
        || lower.contains("cursor")
        || lower == "opencode"
        || lower == "aider"
}

fn collect_agent_pids(info: &LocalProcessInfo, out: &mut Vec<u32>) {
    let exe_name = info
        .executable
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(info.name.as_str());
    if is_agent_exe_name(exe_name) {
        out.push(info.pid);
    }
    for child in info.children.values() {
        collect_agent_pids(child, out);
    }
}

#[cfg(unix)]
fn send_sigterm(pid: u32) -> std::io::Result<()> {
    let rc = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    if rc != 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(unix))]
fn send_sigterm(_pid: u32) -> std::io::Result<()> {
    Ok(())
}

/// Send SIGTERM to every AI-agent process found in the foreground process
/// tree of the given panes, then sleep briefly so each agent can write its
/// resume message. Returns the number of processes that were signaled.
pub fn graceful_kill_agents_in_panes(panes: &[Arc<dyn Pane>]) -> usize {
    let mut pids: Vec<u32> = Vec::new();
    for pane in panes {
        if let Some(info) = pane.get_foreground_process_info(CachePolicy::FetchImmediate) {
            collect_agent_pids(&info, &mut pids);
        }
    }
    pids.sort_unstable();
    pids.dedup();

    if pids.is_empty() {
        return 0;
    }

    let mut sent = 0;
    for pid in &pids {
        match send_sigterm(*pid) {
            Ok(()) => {
                sent += 1;
                log::info!("quit_hooks: sent SIGTERM to agent pid={}", pid);
            }
            Err(err) => {
                log::warn!("quit_hooks: failed to SIGTERM agent pid={}: {:#}", pid, err);
            }
        }
    }

    if sent > 0 {
        std::thread::sleep(GRACE_PERIOD);
    }
    sent
}

/// Convenience: gracefully kill agents across every pane the mux knows about.
pub fn graceful_kill_all_agents(mux: &Mux) -> usize {
    let panes = mux.iter_panes();
    graceful_kill_agents_in_panes(&panes)
}

fn panes_in_window(mux: &Mux, window_id: WindowId) -> Vec<Arc<dyn Pane>> {
    let mut panes: Vec<Arc<dyn Pane>> = Vec::new();
    if let Some(window) = mux.get_window(window_id) {
        for tab in window.iter() {
            for pos in tab.iter_panes_ignoring_zoom() {
                panes.push(pos.pane);
            }
        }
    }
    panes
}

/// Convenience: gracefully kill agents only in panes belonging to the given
/// window.
pub fn graceful_kill_agents_in_window(mux: &Mux, window_id: WindowId) -> usize {
    let panes = panes_in_window(mux, window_id);
    graceful_kill_agents_in_panes(&panes)
}

/// Convenience: gracefully kill agents across every window of the workspace.
pub fn graceful_kill_agents_in_workspace(mux: &Mux, workspace: &str) -> usize {
    let mut panes: Vec<Arc<dyn Pane>> = Vec::new();
    for window_id in mux.iter_windows_in_workspace(workspace) {
        panes.extend(panes_in_window(mux, window_id));
    }
    graceful_kill_agents_in_panes(&panes)
}

#[cfg(test)]
mod test {
    use super::is_agent_exe_name;

    #[test]
    fn recognizes_omp_as_an_agent_process() {
        assert!(is_agent_exe_name("omp"));
        assert!(!is_agent_exe_name("romp"));
    }
}
