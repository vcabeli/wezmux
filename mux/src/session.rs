use crate::domain::SplitSource;
use crate::pane::Pane;
use crate::tab::{PaneNode, SplitRequest, SplitSize};
use crate::Mux;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

const SESSION_VERSION: u32 = 1;
const MAX_SCROLLBACK_LINES: usize = 2_000;

// ---------------------------------------------------------------------------
// Serializable types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct SessionState {
    pub version: u32,
    pub active_workspace: String,
    pub windows: Vec<WindowState>,
    pub sidebar_metadata: HashMap<String, SidebarCacheSerde>,
    pub notifications: Vec<NotificationSerde>,
}

#[derive(Serialize, Deserialize)]
pub struct WindowState {
    pub workspace: String,
    pub title: String,
    pub active_tab_idx: usize,
    pub tabs: Vec<TabState>,
}

#[derive(Serialize, Deserialize)]
pub struct TabState {
    pub title: String,
    pub pane_tree: PaneNode,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SidebarCacheSerde {
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub listening_ports: Vec<u16>,
    pub pull_request: Option<PullRequestSerde>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PullRequestSerde {
    pub number: u64,
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NotificationSerde {
    pub workspace: String,
    pub title: String,
    pub body: String,
    pub age_secs: u64,
    pub unread: bool,
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

pub fn session_dir() -> PathBuf {
    config::DATA_DIR.join("session")
}

fn scrollback_dir() -> PathBuf {
    session_dir().join("scrollback")
}

fn session_json_path() -> PathBuf {
    session_dir().join("session.json")
}

// ---------------------------------------------------------------------------
// PaneNode helpers
// ---------------------------------------------------------------------------

/// Walk PaneNode leaves in order, calling `f` with a mutable reference to each PaneEntry.
fn walk_pane_node_leaves_mut(node: &mut PaneNode, f: &mut dyn FnMut(&mut crate::tab::PaneEntry)) {
    match node {
        PaneNode::Empty => {}
        PaneNode::Split { left, right, .. } => {
            walk_pane_node_leaves_mut(left, f);
            walk_pane_node_leaves_mut(right, f);
        }
        PaneNode::Leaf(entry) => f(entry),
    }
}

/// Get the first (leftmost) leaf entry from a PaneNode tree.
fn first_leaf(node: &PaneNode) -> Option<&crate::tab::PaneEntry> {
    match node {
        PaneNode::Empty => None,
        PaneNode::Leaf(entry) => Some(entry),
        PaneNode::Split { left, .. } => first_leaf(left),
    }
}

// ---------------------------------------------------------------------------
// Save
// ---------------------------------------------------------------------------

fn save_pane_scrollback(pane: &Arc<dyn Pane>, seqid: usize) -> anyhow::Result<()> {
    // Use a background thread with a timeout to avoid blocking shutdown
    // if the terminal mutex is contended (PTY reader threads still running)
    let pane = Arc::clone(pane);
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<Option<String>> {
            let dims = pane.get_dimensions();
            let end = dims.physical_top + dims.viewport_rows as isize;
            let start = (end - MAX_SCROLLBACK_LINES as isize).max(dims.scrollback_top);

            if start >= end {
                return Ok(None);
            }

            let (_first, lines) = pane.get_lines(start..end);
            if lines.is_empty() {
                return Ok(None);
            }

            let text = termwiz_funcs::lines_to_escapes(lines)?;
            if text.trim().is_empty() {
                return Ok(None);
            }

            Ok(Some(text))
        })();
        let _ = tx.send(result);
    });

    // Wait at most 500ms per pane
    match rx.recv_timeout(std::time::Duration::from_millis(500)) {
        Ok(Ok(Some(text))) => {
            let path = scrollback_dir().join(format!("pane_{seqid}.txt.zst"));
            let file = std::fs::File::create(&path)
                .with_context(|| format!("create {}", path.display()))?;
            let mut encoder = zstd::Encoder::new(file, 3)?;
            encoder.write_all(text.as_bytes())?;
            encoder.finish()?;
        }
        Ok(Ok(None)) => {}
        Ok(Err(err)) => {
            log::warn!("Scrollback save for pane seqid {seqid}: {err:#}");
        }
        Err(_timeout) => {
            log::warn!("Scrollback save for pane seqid {seqid} timed out, skipping");
        }
    }
    Ok(())
}


pub fn save_session(mux: &Mux) -> anyhow::Result<()> {
    let dir = session_dir();
    std::fs::create_dir_all(scrollback_dir())?;

    let mut seqid: usize = 0;
    let mut windows = Vec::new();

    for window_id in mux.iter_windows() {
        let window = match mux.get_window(window_id) {
            Some(w) => w,
            None => continue,
        };

        let workspace = window.get_workspace().to_string();
        let title = window.get_title().to_string();
        let active_tab_idx = window.get_active_idx();

        let mut tabs = Vec::new();
        for tab in window.iter() {
            let tab_title = tab.get_title();
            let mut pane_tree = tab.codec_pane_tree();

            // Walk leaves: save scrollback and assign seqids
            walk_pane_node_leaves_mut(&mut pane_tree, &mut |entry| {
                let this_seqid = seqid;
                seqid += 1;

                // Save scrollback for this pane
                if let Some(pane) = mux.get_pane(entry.pane_id) {
                    if let Err(err) = save_pane_scrollback(&pane, this_seqid) {
                        log::warn!("Failed to save scrollback for pane {}: {:#}", entry.pane_id, err);
                    }
                }

                // Replace pane_id with seqid for serialization
                entry.pane_id = this_seqid;
            });

            tabs.push(TabState {
                title: tab_title,
                pane_tree,
            });
        }

        windows.push(WindowState {
            workspace,
            title,
            active_tab_idx,
            tabs,
        });
    }

    // Sidebar cache
    let sidebar_metadata = mux.get_sidebar_cache();

    let state = SessionState {
        version: SESSION_VERSION,
        active_workspace: mux.active_workspace(),
        windows,
        sidebar_metadata,
        notifications: vec![],
    };

    // Atomic write
    let tmp_path = dir.join("session.json.tmp");
    let json = serde_json::to_string_pretty(&state)?;
    std::fs::write(&tmp_path, json.as_bytes())?;
    std::fs::rename(&tmp_path, session_json_path())?;

    log::info!(
        "Session saved: {} windows, {} panes",
        state.windows.len(),
        seqid
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Restore
// ---------------------------------------------------------------------------

fn read_pane_scrollback(seqid: usize) -> anyhow::Result<Option<Vec<u8>>> {
    let path = scrollback_dir().join(format!("pane_{seqid}.txt.zst"));
    if !path.exists() {
        return Ok(None);
    }
    let file = std::fs::File::open(&path)?;
    let mut decoder = zstd::Decoder::new(file)?;
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf)?;
    Ok(Some(buf))
}

fn cwd_from_pane_entry(entry: &crate::tab::PaneEntry) -> Option<String> {
    entry.working_dir.as_ref().and_then(|serde_url| {
        let url: &url::Url = &serde_url.url;
        if url.scheme() == "file" {
            let path = percent_encoding::percent_decode_str(url.path())
                .decode_utf8()
                .ok()?
                .to_string();
            // Verify directory exists
            if std::path::Path::new(&path).is_dir() {
                Some(path)
            } else {
                log::warn!("Session restore: CWD {} no longer exists, using default", path);
                None
            }
        } else {
            None
        }
    })
}

pub async fn restore_session(
    mux: &Arc<Mux>,
    domain: &Arc<dyn crate::domain::Domain>,
) -> anyhow::Result<bool> {
    let json_path = session_json_path();
    if !json_path.exists() {
        return Ok(false);
    }

    let json = std::fs::read_to_string(&json_path)
        .context("read session.json")?;
    let state: SessionState = serde_json::from_str(&json)
        .context("parse session.json")?;

    if state.version != SESSION_VERSION {
        log::warn!(
            "Session version mismatch: expected {}, got {}",
            SESSION_VERSION,
            state.version
        );
        cleanup_session_dir();
        return Ok(false);
    }

    if state.windows.is_empty() {
        cleanup_session_dir();
        return Ok(false);
    }

    for window_state in &state.windows {
        let window_id = {
            let builder = mux.new_empty_window(Some(window_state.workspace.clone()), None);
            *builder
        };

        domain.attach(Some(window_id)).await?;

        for (_tab_idx, tab_state) in window_state.tabs.iter().enumerate() {
            // Get the first leaf to spawn as the tab's initial pane
            let first_entry = match first_leaf(&tab_state.pane_tree) {
                Some(entry) => entry.clone(),
                None => continue,
            };

            let cwd = cwd_from_pane_entry(&first_entry);
            let size = first_entry.size;

            // Set scrollback as banner so it's fed before the PTY reader starts
            set_scrollback_banner(mux, first_entry.pane_id);
            let tab = domain.spawn(size, None, cwd, window_id).await?;
            mux.set_banner(None);

            // Get the initial pane's ID for split restoration
            let initial_pane_id = tab
                .iter_panes_ignoring_zoom()
                .first()
                .map(|p| p.pane.pane_id());

            // Recursively restore split panes
            if let Some(pane_id) = initial_pane_id {
                if let Err(err) = restore_pane_splits(
                    mux, domain, tab.tab_id(), &tab_state.pane_tree, pane_id,
                ).await {
                    log::warn!("Failed to restore splits: {:#}", err);
                }
            }
        }

        // Set active tab
        if let Some(mut window) = mux.get_window_mut(window_id) {
            let idx = window_state.active_tab_idx.min(
                window.len().saturating_sub(1)
            );
            if window.len() > 0 {
                window.set_active_without_saving(idx);
            }
        }
    }

    // Hydrate sidebar cache
    mux.set_sidebar_cache(state.sidebar_metadata);

    // Set active workspace
    mux.set_active_workspace(&state.active_workspace);

    // Clean up session files
    cleanup_session_dir();

    log::info!("Session restored: {} windows", state.windows.len());
    Ok(true)
}

/// Recursively restore split panes within a tab.
///
/// For a Split node, the left subtree's first pane is already spawned (as `pane_id`).
/// We split that pane to create the right subtree's first pane, then recurse into
/// both subtrees.
async fn restore_pane_splits(
    mux: &Arc<Mux>,
    domain: &Arc<dyn crate::domain::Domain>,
    tab_id: crate::tab::TabId,
    node: &PaneNode,
    pane_id: crate::pane::PaneId,
) -> anyhow::Result<()> {
    match node {
        PaneNode::Empty | PaneNode::Leaf(_) => Ok(()),
        PaneNode::Split { left, right, node: split_info } => {
            // Get CWD from the first leaf of the right subtree
            let right_first = first_leaf(right);
            let cwd = right_first.and_then(|e| cwd_from_pane_entry(e));
            let seqid = right_first.map(|e| e.pane_id);

            // Set scrollback banner for the new pane about to be created
            if let Some(seqid) = seqid {
                set_scrollback_banner(mux, seqid);
            }

            // Compute split percentage from saved sizes
            let pct = match split_info.direction {
                crate::tab::SplitDirection::Horizontal => {
                    let total = split_info.first.cols as u32 + split_info.second.cols as u32;
                    if total > 0 { (split_info.second.cols as u32 * 100 / total) as u8 } else { 50 }
                }
                crate::tab::SplitDirection::Vertical => {
                    let total = split_info.first.rows as u32 + split_info.second.rows as u32;
                    if total > 0 { (split_info.second.rows as u32 * 100 / total) as u8 } else { 50 }
                }
            };

            let split_request = SplitRequest {
                direction: split_info.direction,
                target_is_second: true,
                top_level: false,
                size: SplitSize::Percent(pct),
            };

            let new_pane = domain.split_pane(
                SplitSource::Spawn { command: None, command_dir: cwd },
                tab_id,
                pane_id,
                split_request,
            ).await?;

            mux.set_banner(None);
            let new_pane_id = new_pane.pane_id();

            // Recurse into both subtrees
            Box::pin(restore_pane_splits(mux, domain, tab_id, left, pane_id)).await?;
            Box::pin(restore_pane_splits(mux, domain, tab_id, right, new_pane_id)).await?;

            Ok(())
        }
    }
}

/// Set the mux banner to the saved scrollback content for a pane.
/// The banner is written to the PTY parser before the reader starts,
/// avoiding races with shell init output.
fn set_scrollback_banner(mux: &Arc<Mux>, seqid: usize) {
    match read_pane_scrollback(seqid) {
        Ok(Some(data)) => {
            if let Ok(text) = String::from_utf8(data) {
                mux.set_banner(Some(text));
            }
        }
        Ok(None) => {
            mux.set_banner(None);
        }
        Err(err) => {
            log::warn!("Failed to read scrollback for pane seqid {}: {:#}", seqid, err);
            mux.set_banner(None);
        }
    }
}

fn cleanup_session_dir() {
    let dir = session_dir();
    if dir.exists() {
        if let Err(err) = std::fs::remove_dir_all(&dir) {
            log::warn!("Failed to clean up session dir: {:#}", err);
        }
    }
}
