//! Per-workspace metadata for the sidebar: git branch and dirty state, GitHub
//! pull request status and listening ports.
//!
//! Runs in whichever process owns the panes, so it is shared between the GUI
//! and the mux server.

use crate::pane::CachePolicy;
use crate::Mux;
use git2::{Repository, StatusOptions};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

/// What to gather for one workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMetadataRequest {
    /// Directory to resolve the git repository from.
    pub cwd: PathBuf,
    /// Process ids to scope the listening-port scan to.
    pub process_ids: Vec<u32>,
    /// Whether to include the pull request state, which costs a `gh` call.
    pub want_pull_request: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PullRequestStatus {
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestInfo {
    pub number: u64,
    pub status: PullRequestStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMetadataSnapshot {
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub listening_ports: Vec<u16>,
    pub pull_request: Option<PullRequestInfo>,
    /// Whether a git repository was found for the requested cwd. Distinguishes
    /// "no pull request" from "not a repository".
    pub is_git_repo: bool,
}

/// Gather metadata for one workspace. Runs git/gh/lsof, so never call this on
/// a render thread.
pub fn gather(request: &WorkspaceMetadataRequest) -> WorkspaceMetadataSnapshot {
    let repo = Repository::discover(&request.cwd).ok();

    let git_branch = repo.as_ref().and_then(|repo| {
        repo.head().ok().and_then(|head| {
            head.shorthand()
                .map(ToString::to_string)
                .or_else(|| head.target().map(|oid| oid.to_string()[..7].to_string()))
        })
    });

    let git_dirty = repo
        .as_ref()
        .and_then(|repo| repo_has_changes(repo).ok())
        .unwrap_or(false);

    let pull_request = if request.want_pull_request {
        repo.as_ref().and_then(|repo| {
            repo.workdir()
                .or_else(|| repo.path().parent())
                .and_then(load_pull_request)
        })
    } else {
        None
    };

    WorkspaceMetadataSnapshot {
        git_branch,
        git_dirty,
        listening_ports: load_listening_ports(&request.process_ids),
        pull_request,
        is_git_repo: repo.is_some(),
    }
}

/// Resolve `workspace` against this mux's own panes and gather its metadata.
pub fn gather_for_workspace(
    workspace: &str,
    want_pull_request: bool,
) -> Option<WorkspaceMetadataSnapshot> {
    let request = request_for_workspace(workspace, want_pull_request)?;
    Some(gather(&request))
}

/// Build the request for `workspace` from this mux's own panes, or `None` if
/// no pane has reported a working directory. Must run on the mux thread.
pub fn request_for_workspace(
    workspace: &str,
    want_pull_request: bool,
) -> Option<WorkspaceMetadataRequest> {
    let mux = Mux::try_get()?;
    let mut cwd: Option<PathBuf> = None;
    let mut process_ids = BTreeSet::new();

    for window_id in mux.iter_windows_in_workspace(workspace) {
        let Some(window) = mux.get_window(window_id) else {
            continue;
        };

        if cwd.is_none() {
            cwd = window
                .get_active()
                .and_then(|tab| {
                    tab.get_active_pane()
                        .and_then(|pane| pane_cwd(pane.as_ref()))
                })
                .or_else(|| {
                    window.iter().find_map(|tab| {
                        tab.iter_panes()
                            .into_iter()
                            .find_map(|positioned| pane_cwd(positioned.pane.as_ref()))
                    })
                });
        }

        for tab in window.iter() {
            for positioned in tab.iter_panes() {
                if let Some(info) = positioned
                    .pane
                    .get_foreground_process_info(CachePolicy::AllowStale)
                {
                    process_ids.insert(info.pid);
                }
            }
        }
    }

    Some(WorkspaceMetadataRequest {
        cwd: cwd?,
        process_ids: process_ids.into_iter().collect(),
        want_pull_request,
    })
}

fn pane_cwd(pane: &dyn crate::pane::Pane) -> Option<PathBuf> {
    pane.get_current_working_dir(CachePolicy::AllowStale)
        .as_ref()
        .and_then(path_from_cwd_url)
        .or_else(|| {
            pane.get_foreground_process_info(CachePolicy::AllowStale)
                .and_then(|info| {
                    if info.cwd.as_os_str().is_empty() {
                        None
                    } else {
                        Some(info.cwd)
                    }
                })
        })
}

/// Extract the path from an OSC 7 style `file://host/path` URL.
///
/// Unlike `Url::to_file_path`, this accepts a host other than `localhost`: a
/// pane on another machine reports that machine's hostname, and the path is
/// interpreted on the host that produced it.
pub fn path_from_cwd_url(url: &Url) -> Option<PathBuf> {
    if url.scheme() != "file" {
        return None;
    }

    if let Ok(path) = url.to_file_path() {
        return Some(path);
    }

    let decoded = percent_encoding::percent_decode_str(url.path())
        .decode_utf8()
        .ok()?;
    if decoded.is_empty() {
        None
    } else {
        Some(PathBuf::from(decoded.as_ref()))
    }
}

fn repo_has_changes(repo: &Repository) -> anyhow::Result<bool> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(false);

    Ok(!repo.statuses(Some(&mut opts))?.is_empty())
}

fn load_listening_ports(process_ids: &[u32]) -> Vec<u16> {
    if process_ids.is_empty() {
        return vec![];
    }

    let pid_list = process_ids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let output = Command::new("lsof")
        .args([
            "-nP",
            "-iTCP",
            "-sTCP:LISTEN",
            "-a",
            "-p",
            &pid_list,
            "-F",
            "n",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            parse_listening_ports(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(_) | Err(_) => vec![],
    }
}

pub fn parse_listening_ports(output: &str) -> Vec<u16> {
    let mut ports = BTreeSet::new();

    for line in output.lines() {
        let Some(name) = line.strip_prefix('n') else {
            continue;
        };
        let endpoint = name.split("->").next().unwrap_or(name).trim();
        let endpoint = endpoint.split_whitespace().next().unwrap_or(endpoint);
        let Some(port) = endpoint.rsplit(':').next() else {
            continue;
        };
        if let Ok(port) = port.parse::<u16>() {
            ports.insert(port);
        }
    }

    ports.into_iter().collect()
}

#[derive(Debug, Deserialize)]
struct GitHubPullRequestPayload {
    number: u64,
    state: GitHubPullRequestState,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum GitHubPullRequestState {
    Open,
    Closed,
    Merged,
}

fn load_pull_request(repo_root: &Path) -> Option<PullRequestInfo> {
    let output = Command::new("gh")
        .args(["pr", "view", "--json", "number,state,mergedAt"])
        .current_dir(repo_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    parse_pull_request(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_pull_request(output: &str) -> Option<PullRequestInfo> {
    let payload: GitHubPullRequestPayload = serde_json::from_str(output).ok()?;
    let status =
        if payload.merged_at.is_some() || matches!(payload.state, GitHubPullRequestState::Merged) {
            PullRequestStatus::Merged
        } else {
            match payload.state {
                GitHubPullRequestState::Open => PullRequestStatus::Open,
                GitHubPullRequestState::Closed => PullRequestStatus::Closed,
                GitHubPullRequestState::Merged => PullRequestStatus::Merged,
            }
        };

    Some(PullRequestInfo {
        number: payload.number,
        status,
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parses_listening_ports_from_lsof_output() {
        let output = "p1234\nn*:3000\nn127.0.0.1:8080\nn[::1]:8080\nn1.2.3.4:22->5.6.7.8:1234\n";
        assert_eq!(parse_listening_ports(output), vec![22, 3000, 8080]);
    }

    #[test]
    fn parses_pull_request_payload() {
        let payload = r#"{"number":42,"state":"OPEN","mergedAt":null}"#;
        assert_eq!(
            parse_pull_request(payload),
            Some(PullRequestInfo {
                number: 42,
                status: PullRequestStatus::Open
            })
        );

        let merged = r#"{"number":7,"state":"CLOSED","mergedAt":"2024-01-01T00:00:00Z"}"#;
        assert_eq!(
            parse_pull_request(merged),
            Some(PullRequestInfo {
                number: 7,
                status: PullRequestStatus::Merged
            })
        );
    }

    #[test]
    fn extracts_path_from_remote_cwd_url() {
        let local = Url::parse("file:///home/me/project").unwrap();
        assert_eq!(
            path_from_cwd_url(&local),
            Some(PathBuf::from("/home/me/project"))
        );

        let remote = Url::parse("file://build-box/home/me/project%20dir").unwrap();
        assert_eq!(
            path_from_cwd_url(&remote),
            Some(PathBuf::from("/home/me/project dir"))
        );
    }
}
