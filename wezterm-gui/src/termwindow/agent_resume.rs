//! Resume the Claude Code and Codex sessions that were running when Wezmux
//! quit.
//!
//! On quit, `mux::quit_hooks` asks each agent to exit so it prints its
//! resume command ("Resume this session with: claude --resume <id>",
//! "To continue this session, run codex resume <id>"), and the session saves
//! that into the pane's scrollback.  After a restart, each such pane is back
//! at its shell with that command as the last thing it printed.  This finds
//! those panes, asks for extra flags once per agent and types the command,
//! with the flags, into each pane's shell.

use crate::scripting::guiwin::GuiWin;
use crate::termwindow::TermWindow;
use mux::pane::{CachePolicy, Pane};
use mux::Mux;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

/// Rows above the bottom of the viewport to look for a resume command.
const SCAN_ROWS: isize = 300;
/// Non-blank lines allowed after the resume command: the fresh shell's
/// prompt and a few Enter presses.  More means the pane has moved on.
const MAX_TRAILING_LINES: usize = 3;

const SHELLS: &[&str] = &[
    "zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh", "csh", "nu", "xonsh", "elvish",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResumeAgent {
    Claude,
    Codex,
}

impl ResumeAgent {
    fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }
}

/// The resume command an agent printed when it exited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumeCommand {
    pub agent: ResumeAgent,
    pub command: String,
}

/// The resume command at the end of `lines` (a pane's text, oldest first),
/// if the pane printed nothing much after it.
pub fn trailing_resume_command<S: AsRef<str>>(lines: &[S]) -> Option<ResumeCommand> {
    let lines: Vec<&str> = lines
        .iter()
        .map(|l| l.as_ref().trim())
        .filter(|l| !l.is_empty())
        .collect();
    let (idx, found) = lines
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, _)| resume_command_at(&lines, i).map(|cmd| (i, cmd)))?;
    (lines.len() - 1 - idx <= MAX_TRAILING_LINES).then_some(found)
}

/// The resume command whose command text is on `lines[i]`.
fn resume_command_at(lines: &[&str], i: usize) -> Option<ResumeCommand> {
    let line = lines[i];
    let previous = i.checked_sub(1).map(|p| lines[p]).unwrap_or("");

    // Claude Code: "Resume this session with:" then "claude ... --resume <id>"
    if previous.starts_with("Resume this session with")
        && line.starts_with("claude ")
        && line.contains("--resume")
    {
        return Some(ResumeCommand {
            agent: ResumeAgent::Claude,
            command: line.to_string(),
        });
    }

    // Codex: "To continue this session, run:" then "codex resume <id>", or
    // both on one line in older versions.
    const CODEX_LEAD: &str = "To continue this session, run";
    let codex = if previous.starts_with(CODEX_LEAD) && line.starts_with("codex resume ") {
        Some(line)
    } else {
        line.strip_prefix(CODEX_LEAD)
            .map(|rest| rest.trim_start_matches(':').trim())
            .filter(|rest| rest.starts_with("codex resume "))
    };
    codex.map(|command| ResumeCommand {
        agent: ResumeAgent::Codex,
        command: command.to_string(),
    })
}

/// `command` with `flags` appended.
pub fn with_flags(command: &str, flags: &str) -> String {
    let flags = flags.trim();
    if flags.is_empty() {
        command.to_string()
    } else {
        format!("{command} {flags}")
    }
}

/// The flags used last time, per agent.
#[derive(Default, Serialize, Deserialize)]
struct ResumeFlags {
    #[serde(default)]
    claude: String,
    #[serde(default)]
    codex: String,
}

impl ResumeFlags {
    fn path() -> PathBuf {
        config::HOME_DIR
            .join(".config")
            .join("wezmux")
            .join("resume-flags.json")
    }

    fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let path = Self::path();
        let result = (|| -> anyhow::Result<()> {
            std::fs::create_dir_all(path.parent().expect("has a parent"))?;
            std::fs::write(&path, serde_json::to_string_pretty(self)?)?;
            Ok(())
        })();
        if let Err(err) = result {
            log::warn!("Saving {}: {err:#}", path.display());
        }
    }

    fn get(&self, agent: ResumeAgent) -> &str {
        match agent {
            ResumeAgent::Claude => &self.claude,
            ResumeAgent::Codex => &self.codex,
        }
    }

    fn set(&mut self, agent: ResumeAgent, flags: String) {
        match agent {
            ResumeAgent::Claude => self.claude = flags,
            ResumeAgent::Codex => self.codex = flags,
        }
    }
}

/// A pane waiting at its shell with a resume command to run.
pub struct ResumablePane {
    pane: Arc<dyn Pane>,
    workspace: String,
    resume: ResumeCommand,
}

fn is_shell(name: &str) -> bool {
    let name = name.rsplit('/').next().unwrap_or(name);
    let name = name.trim_start_matches('-');
    SHELLS.iter().any(|s| name.eq_ignore_ascii_case(s))
}

fn pane_resume_command(pane: &Arc<dyn Pane>) -> Option<ResumeCommand> {
    // Only type into a shell: never into an agent, an editor or ssh.
    if !is_shell(&pane.get_foreground_process_name(CachePolicy::FetchImmediate)?) {
        return None;
    }
    let dims = pane.get_dimensions();
    let end = dims.physical_top + dims.viewport_rows as isize;
    let start = (end - SCAN_ROWS).max(dims.scrollback_top);
    let lines: Vec<String> = pane
        .get_logical_lines(start..end)
        .into_iter()
        .map(|l| l.logical.as_str().into_owned())
        .collect();
    trailing_resume_command(&lines)
}

impl TermWindow {
    /// Panes across all workspaces, in sidebar order, that can be resumed.
    fn resumable_panes(&self) -> Vec<ResumablePane> {
        let mux = Mux::get();
        let order = self.ordered_workspaces();
        let mut found: Vec<ResumablePane> = mux
            .iter_panes()
            .into_iter()
            .filter_map(|pane| {
                let (_, window_id, _) = mux.resolve_pane_id(pane.pane_id())?;
                let workspace = mux.get_window(window_id)?.get_workspace().to_string();
                let resume = pane_resume_command(&pane)?;
                Some(ResumablePane {
                    pane,
                    workspace,
                    resume,
                })
            })
            .collect();
        found.sort_by_key(|p| {
            (
                order
                    .iter()
                    .position(|w| *w == p.workspace)
                    .unwrap_or(usize::MAX),
                p.pane.pane_id(),
            )
        });
        found
    }

    /// Ask for flags for each agent with sessions to resume, then resume
    /// them all.
    pub fn resume_agents(&mut self) {
        let panes = self.resumable_panes();
        for p in &panes {
            log::info!(
                "resume agents: pane {} in `{}`: {}",
                p.pane.pane_id(),
                p.workspace,
                p.resume.command
            );
        }
        if panes.is_empty() {
            wezterm_toast_notification::persistent_toast_notification(
                "Resume Agents",
                "No pane is back at its shell with a Claude Code or Codex resume command.",
            );
            return;
        }
        self.prompt_resume_flags(panes, ResumeAgent::Claude);
    }

    /// Prompt for `agent`'s flags (skipped when none of `panes` is that
    /// agent), resume its panes, then go on to the next agent.
    fn prompt_resume_flags(&mut self, panes: Vec<ResumablePane>, agent: ResumeAgent) {
        let next = match agent {
            ResumeAgent::Claude => Some(ResumeAgent::Codex),
            ResumeAgent::Codex => None,
        };
        let (mine, rest): (Vec<_>, Vec<_>) =
            panes.into_iter().partition(|p| p.resume.agent == agent);
        if mine.is_empty() {
            if let Some(next) = next {
                self.prompt_resume_flags(rest, next);
            }
            return;
        }

        let mut description = format!(
            "Resume {} {} session{}:\r\n",
            mine.len(),
            agent.label(),
            if mine.len() == 1 { "" } else { "s" }
        );
        for p in &mine {
            description.push_str(&format!(
                "  {:<24} {}\r\n",
                self.sidebar.workspace_configs.display_name(&p.workspace),
                p.resume.command
            ));
        }
        description.push_str(
            "\r\nExtra flags to append (blank for none). Enter resumes, Esc cancels.\r\n",
        );
        let initial = ResumeFlags::load().get(agent).to_string();

        // The overlay's callback must be Send; panes are not, so carry ids.
        let ids: Vec<_> = mine.iter().map(|p| p.pane.pane_id()).collect();
        let rest_ids: Vec<_> = rest.iter().map(|p| p.pane.pane_id()).collect();
        drop((mine, rest));

        let mux = Mux::get();
        let Some(tab) = mux.get_active_tab_for_window(self.mux_window_id) else {
            return;
        };
        let gui_win = GuiWin::new(self);
        let (overlay, future) = crate::overlay::start_overlay(self, &tab, move |_tab_id, term| {
            crate::overlay::prompt::show_text_prompt_overlay(
                term,
                description,
                "flags> ".to_string(),
                Some(initial),
                gui_win,
                Box::new(move |term_window, flags| {
                    let mut saved = ResumeFlags::load();
                    saved.set(agent, flags.trim().to_string());
                    saved.save();

                    // Look the panes up again: one may have moved on while
                    // the prompt was open.
                    let all = term_window.resumable_panes();
                    let (mine, rest): (Vec<_>, Vec<_>) = all
                        .into_iter()
                        .filter(|p| {
                            ids.contains(&p.pane.pane_id()) || rest_ids.contains(&p.pane.pane_id())
                        })
                        .partition(|p| ids.contains(&p.pane.pane_id()));
                    for p in &mine {
                        let line = format!("{}\r", with_flags(&p.resume.command, &flags));
                        if let Err(err) = p.pane.writer().write_all(line.as_bytes()) {
                            log::warn!("Resuming in pane {}: {err:#}", p.pane.pane_id());
                        }
                    }
                    if let Some(next) = next {
                        term_window.prompt_resume_flags(rest, next);
                    }
                }),
            )
        });
        self.assign_overlay(tab.tab_id(), overlay);
        promise::spawn::spawn(future).detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude(cmd: &str) -> Option<ResumeCommand> {
        Some(ResumeCommand {
            agent: ResumeAgent::Claude,
            command: cmd.to_string(),
        })
    }

    fn codex(cmd: &str) -> Option<ResumeCommand> {
        Some(ResumeCommand {
            agent: ResumeAgent::Codex,
            command: cmd.to_string(),
        })
    }

    #[test]
    fn finds_claude_resume_followed_by_prompts() {
        let lines = [
            "➜  repo git:(main) ✗ dclaude --worktree \"wt\" --resume 72ce",
            "Resume this session with:",
            "claude --worktree \"wt\" --resume 72ce   ",
            "",
            "➜  repo git:(main) ✗",
            "➜  repo git:(main) ✗",
            "",
            "",
        ];
        assert_eq!(
            trailing_resume_command(&lines),
            claude("claude --worktree \"wt\" --resume 72ce")
        );
    }

    #[test]
    fn finds_codex_resume_on_two_lines_and_one() {
        let two = [
            "■ out of credits.",
            "To continue this session, run:",
            "  codex resume 01a0c2e0",
            "➜  repo",
        ];
        assert_eq!(
            trailing_resume_command(&two),
            codex("codex resume 01a0c2e0")
        );
        let one = ["To continue this session, run codex resume 01a0", "$"];
        assert_eq!(trailing_resume_command(&one), codex("codex resume 01a0"));
    }

    #[test]
    fn takes_the_latest_resume_command() {
        let lines = [
            "To continue this session, run:",
            "codex resume old",
            "$ dclaude",
            "Resume this session with:",
            "claude --resume new",
            "$",
        ];
        assert_eq!(
            trailing_resume_command(&lines),
            claude("claude --resume new")
        );
    }

    #[test]
    fn ignores_a_pane_that_moved_on() {
        let lines = [
            "Resume this session with:",
            "claude --resume abc",
            "$ ls",
            "a b c",
            "$ cd ..",
            "$",
        ];
        assert_eq!(trailing_resume_command(&lines), None);
    }

    #[test]
    fn ignores_the_command_typed_without_the_header() {
        let lines = ["$ claude --resume abc", "$"];
        assert_eq!(trailing_resume_command(&lines), None);
    }

    #[test]
    fn appends_flags() {
        assert_eq!(
            with_flags("claude --resume x", " --dangerously-skip-permissions "),
            "claude --resume x --dangerously-skip-permissions"
        );
        assert_eq!(with_flags("codex resume x", "  "), "codex resume x");
    }

    #[test]
    fn recognizes_shells() {
        assert!(is_shell("zsh"));
        assert!(is_shell("-zsh"));
        assert!(is_shell("/bin/bash"));
        assert!(!is_shell("claude"));
        assert!(!is_shell("ssh"));
        assert!(!is_shell("nvim"));
    }
}
