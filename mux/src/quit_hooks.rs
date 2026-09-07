//! Graceful shutdown of AI-agent processes (claude, codex, omp, cursor,
//! opencode, aider) running inside panes.
//!
//! Request a normal exit before saving scrollback so the conversation ID is
//! visible after Wezmux restarts. Codex needs terminal input for its session
//! footer; SIGTERM bypasses it. Other agents use SIGTERM.

use crate::pane::{CachePolicy, Pane};
use crate::window::WindowId;
use crate::Mux;
use procinfo::LocalProcessInfo;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Allow final PTY output to reach scrollback before it is saved.
const GRACE_PERIOD: Duration = Duration::from_millis(800);
const CODEX_EXIT_TIMEOUT: Duration = Duration::from_secs(3);
const CODEX_INPUT_DELAY: Duration = Duration::from_millis(250);

struct CodexPane {
    pane: Arc<dyn Pane>,
    pid: u32,
    start_time: u64,
    foreground_pid: u32,
}

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

fn executable_name(info: &LocalProcessInfo) -> &str {
    info.executable
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(info.name.as_str())
}

fn is_interactive_codex(info: &LocalProcessInfo) -> bool {
    if !executable_name(info).eq_ignore_ascii_case("codex") || info.argv.is_empty() {
        return false;
    }
    // Find the subcommand without confusing option values (such as a
    // directory called "app") with non-interactive commands.
    let mut args = info.argv.iter().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" | "--version" | "-V" => return false,
            "--" => return true,
            "-c"
            | "--config"
            | "--enable"
            | "--disable"
            | "--remote"
            | "--remote-auth-token-env"
            | "-i"
            | "--image"
            | "-m"
            | "--model"
            | "--local-provider"
            | "-p"
            | "--profile"
            | "-s"
            | "--sandbox"
            | "-C"
            | "--cd"
            | "--add-dir"
            | "-a"
            | "--ask-for-approval" => {
                args.next();
            }
            option if option.starts_with('-') => {}
            command => {
                return !matches!(
                    command,
                    "exec"
                        | "e"
                        | "review"
                        | "app-server"
                        | "mcp-server"
                        | "exec-server"
                        | "agents"
                        | "login"
                        | "logout"
                        | "mcp"
                        | "plugin"
                        | "remote-control"
                        | "app"
                        | "completion"
                        | "update"
                        | "doctor"
                        | "sandbox"
                        | "debug"
                        | "apply"
                        | "queue"
                        | "archive"
                        | "delete"
                        | "migrate-rollouts"
                        | "unarchive"
                        | "cloud"
                        | "features"
                        | "help"
                );
            }
        }
    }
    true
}

fn collect_agent_pids<'a>(
    info: &'a LocalProcessInfo,
    pids: &mut Vec<u32>,
    codex: &mut Vec<&'a LocalProcessInfo>,
) {
    if is_interactive_codex(info) {
        codex.push(info);
        // Let the TUI shut down its own children. Signaling its backend now
        // can prevent the session footer from being printed.
        return;
    }
    if is_agent_exe_name(executable_name(info)) {
        pids.push(info.pid);
    }
    for child in info.children.values() {
        collect_agent_pids(child, pids, codex);
    }
}

fn contains_process(info: &LocalProcessInfo, pid: u32, start_time: u64) -> bool {
    (info.pid == pid
        && info.start_time == start_time
        && !matches!(
            info.status,
            procinfo::LocalProcessStatus::Zombie | procinfo::LocalProcessStatus::Dead
        ))
        || info
            .children
            .values()
            .any(|child| contains_process(child, pid, start_time))
}

impl CodexPane {
    fn is_foreground(&self) -> bool {
        self.pane
            .get_foreground_process_info(CachePolicy::FetchImmediate)
            .is_some_and(|info| {
                info.pid == self.foreground_pid
                    && contains_process(&info, self.pid, self.start_time)
            })
    }

    fn send_key(&self, key: u8) -> std::io::Result<()> {
        // Codex restores cooked mode before its process disappears. Avoid
        // delivering a signal during that final cleanup. Use Ctrl-C for both
        // steps: unlike Ctrl-D, it cannot close an interactive shell if the
        // foreground process changes between this check and the write.
        if self.is_foreground() && self.pane.is_tty_in_raw_mode() {
            let mut writer = self.pane.writer();
            writer.write_all(&[key])?;
            writer.flush()?;
        }
        Ok(())
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

/// Request an exit from agents in the foreground process trees, then allow
/// their final output to reach scrollback. Returns the number requested.
pub fn graceful_kill_agents_in_panes(panes: &[Arc<dyn Pane>]) -> usize {
    let mut pids = Vec::new();
    let mut codex_panes = Vec::new();
    for pane in panes {
        if let Some(info) = pane.get_foreground_process_info(CachePolicy::FetchImmediate) {
            let mut codex = Vec::new();
            collect_agent_pids(&info, &mut pids, &mut codex);
            // Keyboard input belongs to the pane, so send it only once even
            // when the process tree contains more than one Codex process.
            if let Some(codex) = codex.first() {
                codex_panes.push(CodexPane {
                    pane: Arc::clone(pane),
                    pid: codex.pid,
                    start_time: codex.start_time,
                    foreground_pid: info.pid,
                });
            }
        }
    }
    pids.sort_unstable();
    pids.dedup();
    codex_panes.sort_unstable_by_key(|codex| codex.pid);
    codex_panes.dedup_by_key(|codex| codex.pid);

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

    if !codex_panes.is_empty() {
        for codex in &codex_panes {
            // Clear a draft / interrupt a running turn before requesting
            // exit. An idle Codex may already exit on this Ctrl-C.
            if let Err(err) = codex.send_key(0x03) {
                log::warn!("quit_hooks: Codex interrupt pid={}: {err:#}", codex.pid);
            }
        }
        std::thread::sleep(CODEX_INPUT_DELAY);
        for codex in &codex_panes {
            if let Err(err) = codex.send_key(0x03) {
                log::warn!("quit_hooks: Codex exit pid={}: {err:#}", codex.pid);
            }
        }
        // One shared deadline, rather than a separate timeout for each pane.
        let deadline = Instant::now() + CODEX_EXIT_TIMEOUT;
        while codex_panes.iter().any(CodexPane::is_foreground) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        for codex in &codex_panes {
            if codex.is_foreground() {
                log::warn!(
                    "quit_hooks: Codex pid={} did not exit; sending SIGTERM",
                    codex.pid
                );
                if let Err(err) = send_sigterm(codex.pid) {
                    log::warn!(
                        "quit_hooks: failed to SIGTERM Codex pid={}: {err:#}",
                        codex.pid
                    );
                }
            }
        }
        sent += codex_panes.len();
    }

    if sent > 0 {
        // PTY reader/parser threads keep running while the GUI waits. Give
        // them time to apply the footer before the caller saves scrollback.
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
    use super::*;

    fn process(
        pid: u32,
        exe: &str,
        args: &[&str],
        children: Vec<LocalProcessInfo>,
    ) -> LocalProcessInfo {
        LocalProcessInfo {
            pid,
            ppid: 0,
            name: exe.to_string(),
            executable: format!("/usr/local/bin/{exe}").into(),
            argv: std::iter::once(exe)
                .chain(args.iter().copied())
                .map(str::to_string)
                .collect(),
            cwd: Default::default(),
            status: procinfo::LocalProcessStatus::Run,
            start_time: 123,
            #[cfg(windows)]
            console: 0,
            children: children
                .into_iter()
                .map(|child| (child.pid, child))
                .collect(),
        }
    }

    #[test]
    fn recognizes_omp_as_an_agent_process() {
        assert!(is_agent_exe_name("omp"));
        assert!(!is_agent_exe_name("romp"));
    }

    #[test]
    fn codex_launcher_uses_keyboard_exit_without_signaling_its_backend() {
        let backend = process(3, "codex", &["app-server"], vec![]);
        let tui = process(2, "codex", &["resume", "session-id"], vec![backend]);
        let launcher = process(1, "node", &["/opt/homebrew/bin/codex"], vec![tui]);
        let mut pids = vec![];
        let mut codex = vec![];
        collect_agent_pids(&launcher, &mut pids, &mut codex);
        assert!(pids.is_empty());
        assert_eq!(
            codex.iter().map(|info| info.pid).collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn noninteractive_codex_and_other_agents_use_signals() {
        for (exe, args) in [
            ("codex", vec!["exec", "run tests"]),
            ("codex", vec!["-c", "model=test", "exec", "run tests"]),
            ("codex", vec!["app-server"]),
            ("codex", vec!["mcp-server"]),
            ("claude", vec![]),
            ("omp", vec![]),
        ] {
            let info = process(2, exe, &args, vec![]);
            let mut pids = vec![];
            let mut codex = vec![];
            collect_agent_pids(&info, &mut pids, &mut codex);
            assert_eq!(pids, vec![2], "{exe} {args:?}");
            assert!(codex.is_empty(), "{} {:?}", exe, args);
        }
    }

    #[test]
    fn interactive_codex_modes_use_keyboard_exit() {
        for args in [
            vec![],
            vec!["--no-alt-screen"],
            vec!["--cd", "app", "--disable", "apps"],
            vec!["--", "review"],
            vec!["resume", "session-id"],
            vec!["fork", "--last"],
        ] {
            assert!(is_interactive_codex(&process(2, "codex", &args, vec![])));
        }
    }

    #[test]
    fn exited_or_replaced_codex_is_not_a_keyboard_target() {
        let codex = process(2, "codex", &[], vec![]);
        let foreground = process(1, "node", &[], vec![codex.clone()]);
        assert!(contains_process(&foreground, 2, codex.start_time));
        assert!(!contains_process(&foreground, 2, codex.start_time + 1));
        let shell = process(1, "zsh", &[], vec![]);
        assert!(!contains_process(&shell, 2, codex.start_time));
        let mut zombie = codex;
        zombie.status = procinfo::LocalProcessStatus::Zombie;
        assert!(!contains_process(&zombie, 2, zombie.start_time));
    }
}
