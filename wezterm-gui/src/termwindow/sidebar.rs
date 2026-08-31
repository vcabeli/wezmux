use config::{ConfigHandle, DimensionContext, TermConfig};
use git2::{Repository, StatusOptions};
use mux::pane::{CachePolicy, PaneId};
use mux::Mux;
use promise::spawn::{spawn, spawn_into_new_thread};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;
use window::WindowOps;

use crate::frontend::WorkspaceSwitcher;
use crate::scripting::guiwin::GuiWin;
use crate::spawn::SpawnWhere;
use procinfo::LocalProcessInfo;
use std::sync::Arc;
use std::time::{Duration, Instant};

const SIDEBAR_METADATA_COALESCE_DELAY: Duration = Duration::from_millis(200);
const SIDEBAR_PULL_REQUEST_REFRESH_INTERVAL: Duration = Duration::from_secs(60);

/// How long the process table snapshot backing `PaneProcessFacts` may be
/// reused.  The refresh itself is coalesced to
/// `SIDEBAR_METADATA_COALESCE_DELAY`, so this only needs to be short enough
/// that consecutive refreshes don't share a snapshot.
const PANE_PROCESS_FACTS_TTL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Default)]
pub struct SidebarState {
    pub visible: bool,
    pub hovered_workspace: Option<String>,
    pub override_width: Option<f32>,
    pub scroll_offset: f32,
    pub metadata: HashMap<String, WorkspaceMetadata>,
    /// The last refresh request built by `sidebar_entries`, so we can tell
    /// when the thing we'd ask the refresh thread for has actually changed.
    metadata_request: Option<SidebarRefreshRequest>,
    pub metadata_refresh_in_flight: bool,
    pub next_metadata_refresh: Option<Instant>,
    /// Cached sidebar entries from previous frame.
    cached_entries: Option<Vec<WorkspaceEntry>>,
    /// Workspace count at last cache build (invalidate on structural change).
    cached_workspace_count: usize,
    /// Active workspace at last cache build.
    cached_active_workspace: String,
    /// Hovered workspace at last cache build.
    cached_hovered_workspace: Option<String>,
    /// Agent status store generation at last cache build.
    cached_agent_status_generation: u64,
    /// Facts derived from each pane's foreground process tree.
    ///
    /// Filled in by the metadata refresh thread; the render path only reads
    /// it.  See [`PaneProcessFacts`].
    pane_process_facts: HashMap<PaneId, PaneProcessFacts>,
    /// Bumped whenever `pane_process_facts` actually changes, so that cached
    /// entries invalidate exactly when the facts they were built from move.
    pane_facts_generation: u64,
    /// `pane_facts_generation` at last cache build.
    cached_pane_facts_generation: u64,
    /// Last detected agent type per pane — survives transient process detection failures.
    pub last_known_agents: HashMap<PaneId, AgentType>,
    /// Workspace targeted by the currently open native context menu, if any.
    pub context_menu_workspace: Option<String>,
    /// Per-workspace customizations (display name, accent color, ordering).
    pub workspace_configs: crate::termwindow::workspace_config::WorkspaceConfigs,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceMetadata {
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub listening_ports: Vec<u16>,
    pub pull_request: Option<WorkspacePullRequest>,
    pull_request_checked_for_branch: Option<String>,
    pull_request_checked_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspacePullRequestStatus {
    Open,
    Merged,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePullRequest {
    pub number: u64,
    pub status: WorkspacePullRequestStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    ClaudeCode,
    Codex,
    Omp,
    Cursor,
    OpenCode,
    Aider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    NeedsInput,
    Idle,
    Working,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentInfo {
    pub agent_type: AgentType,
    pub display_name: String,
    pub conversation_title: Option<String>,
    pub status: AgentStatus,
    pub status_message: Option<String>,
    /// Number of subagents running within this agent session.
    pub subagent_count: u32,
    /// Number of background tasks (Bash run_in_background, Monitor) tracked
    /// by the agent session.
    pub background_tasks_count: u32,
    /// Number of scheduled tasks (`/loop`, CronCreate) tracked by the agent
    /// session.
    pub session_crons_count: u32,
}

/// What the sidebar knows about a pane's foreground process.
///
/// Deriving these means walking the whole system process table to build the
/// tree rooted at the pane's foreground pid — far too expensive to do while
/// painting, and it takes the kernel's global process list lock, so doing it
/// on the render thread also made frame times hostage to unrelated processes
/// forking.  It happens on the metadata refresh thread instead; the render
/// path only reads the result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaneProcessFacts {
    /// False when the tree could not be resolved at all — eg: the foreground
    /// process exited between us reading its pid and walking the table.
    ///
    /// This must stay distinguishable from "resolved, and it is not an
    /// agent", because only the latter means an agent genuinely exited.
    /// Treating a transient failure as an exit wipes the sidebar preview.
    pub resolved: bool,
    /// pid of the foreground process, for the listening-ports lookup.
    pub pid: Option<u32>,
    /// Agent detected anywhere in the foreground process tree.
    pub agent_type: Option<AgentType>,
    /// cwd of the foreground process — a fallback for the card's path label
    /// when OSC 7 didn't provide one.
    pub cwd: Option<PathBuf>,
    /// An executable name from the tree, for the card's icon.
    pub foreground_process_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceMetadataTarget {
    workspace_name: String,
    cwd_path: PathBuf,
    /// Panes belonging to this workspace.  The refresh thread resolves these
    /// to pids via [`PaneProcessFacts`] and hands them to the ports lookup.
    pane_ids: Vec<PaneId>,
}

/// Everything the refresh thread needs, gathered cheaply on the render thread.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SidebarRefreshRequest {
    targets: Vec<WorkspaceMetadataTarget>,
    /// Every pane and the pid of its foreground process.  Reading the pid is
    /// cheap; resolving what it *is* is not, so that happens off-thread.
    pane_leaders: Vec<(PaneId, u32)>,
}

/// Result of a refresh: per-workspace metadata plus the per-pane facts that
/// were resolved along the way.
struct SidebarRefreshResult {
    metadata: HashMap<String, WorkspaceMetadata>,
    pane_facts: HashMap<PaneId, PaneProcessFacts>,
}

impl SidebarState {
    pub fn new(config: &ConfigHandle) -> Self {
        // Hydrate metadata from restored session cache if available
        let metadata = if let Some(mux) = mux::Mux::try_get() {
            let cache = mux.get_sidebar_cache();
            if !cache.is_empty() {
                cache
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            WorkspaceMetadata {
                                git_branch: v.git_branch,
                                git_dirty: v.git_dirty,
                                listening_ports: v.listening_ports,
                                pull_request: v.pull_request.map(|pr| WorkspacePullRequest {
                                    number: pr.number,
                                    status: match pr.status.as_str() {
                                        "Merged" => WorkspacePullRequestStatus::Merged,
                                        "Closed" => WorkspacePullRequestStatus::Closed,
                                        _ => WorkspacePullRequestStatus::Open,
                                    },
                                }),
                                pull_request_checked_for_branch: None,
                                pull_request_checked_at: None,
                            },
                        )
                    })
                    .collect()
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        Self {
            visible: config.sidebar.visible,
            hovered_workspace: None,
            override_width: None,
            scroll_offset: 0.0,
            metadata,
            metadata_request: None,
            metadata_refresh_in_flight: false,
            next_metadata_refresh: None,
            cached_entries: None,
            cached_workspace_count: 0,
            cached_active_workspace: String::new(),
            cached_hovered_workspace: None,
            cached_agent_status_generation: 0,
            pane_process_facts: HashMap::new(),
            pane_facts_generation: 0,
            cached_pane_facts_generation: 0,
            last_known_agents: HashMap::new(),
            context_menu_workspace: None,
            workspace_configs: crate::termwindow::workspace_config::WorkspaceConfigs::load(),
        }
    }

    pub fn invalidate_cache(&mut self) {
        self.cached_entries = None;
    }

    pub fn pixel_width(
        &self,
        config: &ConfigHandle,
        dpi: f32,
        pixel_max: f32,
        pixel_cell: f32,
    ) -> usize {
        if !self.visible {
            return 0;
        }

        if let Some(w) = self.override_width {
            return w.round() as usize;
        }

        configured_pixel_width(
            config,
            DimensionContext {
                dpi,
                pixel_max,
                pixel_cell,
            },
        )
    }

    /// Ask for a refresh no later than `delay` from now.
    ///
    /// Coalescing, not debouncing: an already-pending earlier deadline wins.
    /// Overwriting it would let a continuously-outputting pane — every chunk
    /// of output schedules a refresh — push the deadline back indefinitely and
    /// starve the refresh exactly when there is most to report.
    pub fn schedule_metadata_refresh(&mut self, delay: Duration) {
        let deadline = Instant::now() + delay;
        self.next_metadata_refresh = Some(match self.next_metadata_refresh {
            Some(pending) => pending.min(deadline),
            None => deadline,
        });
    }

    pub fn schedule_metadata_refresh_immediate(&mut self) {
        self.schedule_metadata_refresh(Duration::ZERO);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceEntry {
    pub name: String,
    pub title: String,
    pub cwd: Option<String>,
    pub cwd_path: Option<PathBuf>,
    pub git_branch: Option<String>,
    pub git_dirty: bool,
    pub listening_ports: Vec<u16>,
    pub pull_request: Option<WorkspacePullRequest>,
    pub latest_notification: Option<String>,
    pub unread_count: u32,
    pub tab_count: usize,
    pub pane_count: usize,
    pub is_active: bool,
    pub is_hovered: bool,
    pub agent: Option<AgentInfo>,
    /// Foreground process name (e.g. "nvim", "node") for icon display.
    pub foreground_process_name: Option<String>,
    /// Custom accent color from workspace config (hex string like "#ff6b6b").
    pub accent_color: Option<String>,
    /// Custom emoji from workspace config, shown next to the title.
    pub emoji: Option<String>,
    /// User-supplied nickname. Rendered as a centered header above the title
    /// line so it stays visible even when an agent overrides the title.
    pub display_name: Option<String>,
}

pub fn configured_pixel_width(config: &ConfigHandle, context: DimensionContext) -> usize {
    if !config.sidebar.visible {
        return 0;
    }
    config.sidebar.width.evaluate_as_pixels(context).round() as usize
}

impl crate::TermWindow {
    pub fn effective_use_fancy_tab_bar(&self) -> bool {
        self.sidebar.visible || self.config.use_fancy_tab_bar
    }

    pub fn effective_tab_bar_config(&self) -> config::Config {
        let mut config = (*self.config).clone();
        if self.sidebar.visible {
            config.use_fancy_tab_bar = true;
        }
        config
    }

    pub fn sidebar_pixel_width(&self) -> f32 {
        self.sidebar.pixel_width(
            &self.config,
            self.dimensions.dpi as f32,
            self.dimensions.pixel_width as f32,
            self.render_metrics.cell_size.width as f32,
        ) as f32
    }

    /// Returns workspace names in the user's configured display order.
    /// This must be used everywhere that maps indices to workspaces
    /// (Cmd+1-9, relative switching) so shortcuts match the sidebar.
    pub fn ordered_workspaces(&self) -> Vec<String> {
        let mux = Mux::get();
        let raw = mux.iter_workspaces();
        self.sidebar.workspace_configs.apply_order(&raw)
    }

    pub fn tab_bar_pixel_bounds(&self) -> (f32, f32) {
        let border = self.get_os_border();
        let left = border.left.get() as f32 + self.sidebar_pixel_width();
        let right = self.dimensions.pixel_width as f32 - border.right.get() as f32;
        (left, (right - left).max(0.0))
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar.visible = !self.sidebar.visible;
        self.sidebar.hovered_workspace = None;
        if self.sidebar.visible {
            self.sidebar.schedule_metadata_refresh_immediate();
        }
        self.fancy_tab_bar.take();
        self.invalidate_fancy_tab_bar();
        self.invalidate_modal();

        if let Some(window) = self.window.as_ref().cloned() {
            let dimensions = self.dimensions;
            self.apply_dimensions(&dimensions, None, &window);
            window.invalidate();
        }
    }

    pub fn sidebar_entries(&mut self) -> Vec<WorkspaceEntry> {
        let mux = Mux::get();
        let active_workspace = mux.active_workspace();
        let hovered = self.sidebar.hovered_workspace.clone();
        let workspace_count = mux.iter_workspaces().len();

        // Fast path: reuse cached entries if nothing changed.
        // Check notification counts to detect mark-as-read events.
        let cached_unread_match = self
            .sidebar
            .cached_entries
            .as_ref()
            .map(|entries| {
                entries
                    .iter()
                    .all(|e| mux.unread_notification_count_for_workspace(&e.name) == e.unread_count)
            })
            .unwrap_or(false);
        let cached_metadata_match = self
            .sidebar
            .cached_entries
            .as_ref()
            .map(|entries| {
                entries.iter().all(|e| {
                    self.sidebar.metadata.get(&e.name).map_or(true, |m| {
                        m.git_branch == e.git_branch
                            && m.git_dirty == e.git_dirty
                            && m.listening_ports == e.listening_ports
                            && m.pull_request == e.pull_request
                    })
                })
            })
            .unwrap_or(true);
        let agent_generation = mux.agent_status_generation();
        let pane_facts_generation = self.sidebar.pane_facts_generation;
        let structural_change = workspace_count != self.sidebar.cached_workspace_count
            || active_workspace != self.sidebar.cached_active_workspace
            || hovered != self.sidebar.cached_hovered_workspace
            || !cached_unread_match
            || !cached_metadata_match
            || agent_generation != self.sidebar.cached_agent_status_generation
            || pane_facts_generation != self.sidebar.cached_pane_facts_generation;
        if !structural_change {
            if let Some(cached) = self.sidebar.cached_entries.clone() {
                // Launching a refresh must not be conditional on the cards
                // needing a rebuild: the facts they were built from can go
                // stale — an agent exits, a branch changes — with nothing
                // here changing to notice it.  Reuse the last request; its
                // pane list is at most one rebuild out of date.
                if let Some(request) = self.sidebar.metadata_request.clone() {
                    self.maybe_refresh_sidebar_metadata(&request);
                }
                return cached;
            }
        }

        let hovered = hovered.as_deref();
        let mut refresh_targets = vec![];
        // Collected for the refresh thread.  Reading a pane's foreground pid
        // is cheap; resolving the process tree behind it is not, so nothing
        // below this point touches the process table.
        let mut pane_leaders: Vec<(PaneId, u32)> = vec![];

        let mux_workspaces = mux.iter_workspaces();
        let ordered_names = self.sidebar.workspace_configs.apply_order(&mux_workspaces);
        let entries: Vec<_> = ordered_names
            .into_iter()
            .map(|name| {
                let mut title = name.clone();
                let mut cwd = None;
                let mut cwd_path = None;
                let mut tab_count = 0;
                let mut pane_count = 0;
                let mut workspace_pane_ids = BTreeSet::new();
                let mut active_pane_id: Option<mux::pane::PaneId> = None;

                for window_id in mux.iter_windows_in_workspace(&name) {
                    if let Some(window) = mux.get_window(window_id) {
                        tab_count += window.len();
                        if title == name {
                            if let Some(tab) = window.get_active() {
                                title = sidebar_title_from_tab(tab.as_ref())
                                    .unwrap_or_else(|| name.clone());
                                if active_pane_id.is_none() {
                                    if let Some(pane) = tab.get_active_pane() {
                                        active_pane_id = Some(pane.pane_id());
                                    }
                                }
                            }
                        }
                        if cwd.is_none() || cwd_path.is_none() {
                            if let Some((label, path)) = window
                                .get_active()
                                .and_then(|tab| {
                                    sidebar_context_from_active_tab(
                                        tab.as_ref(),
                                        &self.sidebar.pane_process_facts,
                                    )
                                })
                                .or_else(|| {
                                    window.iter().find_map(|tab| {
                                        sidebar_context_from_tab(
                                            tab.as_ref(),
                                            &self.sidebar.pane_process_facts,
                                        )
                                    })
                                })
                            {
                                cwd = label;
                                cwd_path = path;
                            }
                        }
                        for tab in window.iter() {
                            pane_count += tab.count_panes().unwrap_or(0);
                            for positioned in tab.iter_panes() {
                                let pane_id = positioned.pane.pane_id();
                                workspace_pane_ids.insert(pane_id);
                                if let Some(leader) = positioned.pane.foreground_process_leader() {
                                    pane_leaders.push((pane_id, leader));
                                }
                            }
                        }
                    }
                }

                // Read-only: whatever the last refresh resolved for this
                // pane.  Absent until the first refresh completes, which the
                // `last_known_agents` fallback in `build_agent_info` covers.
                let active_pane_facts =
                    active_pane_id.and_then(|id| self.sidebar.pane_process_facts.get(&id));

                let metadata = self
                    .sidebar
                    .metadata
                    .get(&name)
                    .cloned()
                    .unwrap_or_default();
                let latest_notification =
                    mux.latest_notification_for_workspace(&name)
                        .and_then(|notification| {
                            if notification.body.is_empty() {
                                if notification.title.is_empty() {
                                    None
                                } else {
                                    Some(notification.title)
                                }
                            } else {
                                Some(notification.body)
                            }
                        });
                let unread_count = mux.unread_notification_count_for_workspace(&name);

                // Reaping a departed agent is a *write*, so it does not
                // happen here — see `reap_departed_agents`, which runs when a
                // refresh lands.  Building entries must stay read-only: this
                // function's cache validity is derived from the very state it
                // would be mutating, so a write here makes each rebuild
                // invalidate its own cache and repaint forever.
                let agent = build_agent_info(
                    active_pane_facts.and_then(|facts| facts.agent_type),
                    active_pane_id,
                    active_pane_id.and_then(|id| self.sidebar.last_known_agents.get(&id).copied()),
                );

                if let Some(cwd_path) = cwd_path.as_ref() {
                    refresh_targets.push(WorkspaceMetadataTarget {
                        workspace_name: name.clone(),
                        cwd_path: cwd_path.clone(),
                        pane_ids: workspace_pane_ids.into_iter().collect(),
                    });
                }

                // Pull the user's nickname (if any) — rendered as its own
                // centered header line in the card, separate from `title`,
                // so it remains visible even when an agent overrides the title.
                let display_title = self.sidebar.workspace_configs.display_name(&name);
                let display_name = if display_title != name {
                    Some(display_title)
                } else {
                    None
                };
                let accent_color = self.sidebar.workspace_configs.accent_color(&name);
                let emoji = self.sidebar.workspace_configs.emoji(&name);

                let foreground_process_name =
                    active_pane_facts.and_then(|facts| facts.foreground_process_name.clone());

                WorkspaceEntry {
                    is_active: active_workspace == name,
                    is_hovered: hovered == Some(name.as_str()),
                    name: name.clone(),
                    title,
                    cwd,
                    cwd_path: cwd_path.clone(),
                    git_branch: metadata.git_branch,
                    git_dirty: metadata.git_dirty,
                    listening_ports: metadata.listening_ports,
                    pull_request: metadata.pull_request,
                    latest_notification,
                    unread_count,
                    tab_count,
                    pane_count,
                    agent,
                    foreground_process_name,
                    accent_color,
                    emoji,
                    display_name,
                }
            })
            .collect();

        // Sorted so that a differing iteration order can't masquerade as a
        // change and trigger a pointless refresh.
        pane_leaders.sort_unstable();
        pane_leaders.dedup();
        let request = SidebarRefreshRequest {
            targets: refresh_targets,
            pane_leaders,
        };

        // A changed request includes a pane whose foreground process changed,
        // which is exactly when its facts need re-resolving.
        let request_changed = self.sidebar.metadata_request.as_ref() != Some(&request);
        let missing_metadata = request
            .targets
            .iter()
            .any(|target| !self.sidebar.metadata.contains_key(&target.workspace_name));
        let mut scheduled_refresh = false;

        if request_changed {
            if missing_metadata {
                self.sidebar.schedule_metadata_refresh_immediate();
            } else {
                self.sidebar
                    .schedule_metadata_refresh(SIDEBAR_METADATA_COALESCE_DELAY);
            }
            scheduled_refresh = true;
        } else if missing_metadata {
            self.sidebar.schedule_metadata_refresh_immediate();
            scheduled_refresh = true;
        }

        if scheduled_refresh {
            if let Some(window) = self.window.as_ref() {
                window.invalidate();
            }
        }

        self.maybe_refresh_sidebar_metadata(&request);
        self.sidebar.metadata_request = Some(request);

        // Update cache
        self.sidebar.cached_entries = Some(entries.clone());
        self.sidebar.cached_workspace_count = workspace_count;
        self.sidebar.cached_active_workspace = active_workspace;
        self.sidebar.cached_hovered_workspace = self.sidebar.hovered_workspace.clone();
        self.sidebar.cached_agent_status_generation = agent_generation;
        self.sidebar.cached_pane_facts_generation = pane_facts_generation;

        entries
    }

    /// Reconcile the agent caches with what the panes are actually running.
    ///
    /// This is the write half of what `sidebar_entries` used to do inline.
    /// It runs when a refresh lands, so that building entries can stay a pure
    /// read — the entry cache's validity is derived from the agent status
    /// generation, so a write from inside the rebuild made every rebuild
    /// invalidate its own cache and repaint forever.
    fn reap_departed_agents(&mut self) {
        let mux = Mux::get();

        for (pane_id, facts) in &self.sidebar.pane_process_facts {
            match facts.agent_type {
                // Remember it, so that a later failure to resolve this pane
                // doesn't blank a card that is still running an agent.
                Some(agent_type) => {
                    self.sidebar.last_known_agents.insert(*pane_id, agent_type);
                }
                // We could see the process and it is not an agent, so the
                // agent genuinely exited.
                None if facts.resolved => {
                    self.sidebar.last_known_agents.remove(pane_id);
                    mux.remove_agent_status(*pane_id);
                }
                // Unresolved: transient, leave the caches alone.
                None => {}
            }
        }
    }

    pub fn schedule_sidebar_metadata_refresh(&mut self) {
        self.sidebar
            .schedule_metadata_refresh(SIDEBAR_METADATA_COALESCE_DELAY);
        self.invalidate_fancy_tab_bar();
        if let Some(window) = self.window.as_ref() {
            window.invalidate();
        }
    }

    pub fn jump_to_unread_notification(&mut self) {
        let mux = Mux::get();
        if let Some(pane_id) = mux.most_recent_unread_notification_pane() {
            // Find which workspace this pane belongs to and switch to it
            if let Some((_domain_id, window_id, _tab_id)) = mux.resolve_pane_id(pane_id) {
                if let Some(window) = mux.get_window(window_id) {
                    let workspace = window.get_workspace().to_string();
                    let switcher = WorkspaceSwitcher::new(&workspace);
                    mux.set_active_workspace(&workspace);
                    switcher.do_switch();
                }
            }
        }
    }

    /// Handle a native context menu selection.
    /// Tags: 1=Rename, 2=MoveUp, 3=MoveDown, 4=MoveToTop, 5=MoveToBottom,
    ///        6=Close, 7=EmojiPicker, 100=ColorReset, 101-108=Color swatches
    pub fn handle_context_menu_selection(&mut self, tag: usize, window: &::window::Window) {
        let workspace = match self.sidebar.context_menu_workspace.take() {
            Some(name) => name,
            None => return,
        };

        const COLOR_HEXES: &[&str] = &[
            "#ff6b6b", "#ffa94d", "#ffd43b", "#69db7c", "#38d9a9", "#4dabf7", "#b197fc", "#f783ac",
        ];

        match tag {
            1 => {
                self.show_workspace_nickname_prompt(workspace.clone());
                // The prompt handles its own save + invalidate when the user submits.
                return;
            }
            2 => {
                let all = Mux::get().iter_workspaces();
                self.sidebar.workspace_configs.move_up(&workspace, &all);
            }
            3 => {
                let all = Mux::get().iter_workspaces();
                self.sidebar.workspace_configs.move_down(&workspace, &all);
            }
            4 => {
                let all = Mux::get().iter_workspaces();
                self.sidebar.workspace_configs.move_to_top(&workspace, &all);
            }
            5 => {
                let all = Mux::get().iter_workspaces();
                self.sidebar
                    .workspace_configs
                    .move_to_bottom(&workspace, &all);
            }
            6 => {
                self.close_workspace_by_name(&workspace);
            }
            7 => {
                self.show_workspace_emoji_picker(workspace.clone());
                // Modal handles its own save + invalidate on selection.
                return;
            }
            100 => {
                self.sidebar
                    .workspace_configs
                    .set_accent_color(&workspace, None);
            }
            101..=108 => {
                let idx = tag - 101;
                if let Some(hex) = COLOR_HEXES.get(idx) {
                    self.sidebar
                        .workspace_configs
                        .set_accent_color(&workspace, Some(hex.to_string()));
                }
            }
            _ => {}
        }

        if let Err(e) = self.sidebar.workspace_configs.save() {
            log::error!("Failed to save workspace configs: {:#}", e);
        }
        self.sidebar.invalidate_cache();
        window.invalidate();
    }

    /// Close workspace by name (used by context menu handler).
    fn close_workspace_by_name(&mut self, workspace: &str) {
        let mux = Mux::get();
        if mux.active_workspace() == workspace {
            // Switch to the next workspace in display order
            let ordered = self.ordered_workspaces();
            let idx = ordered.iter().position(|w| w == workspace).unwrap_or(0);
            let next = if idx + 1 < ordered.len() {
                Some(ordered[idx + 1].clone())
            } else if idx > 0 {
                Some(ordered[idx - 1].clone())
            } else {
                None
            };
            if let Some(ref next_ws) = next {
                crate::frontend::front_end().switch_workspace(next_ws);
            } else {
                return; // Only workspace, don't close
            }
        }
        mux::quit_hooks::graceful_kill_agents_in_workspace(&mux, workspace);
        let window_ids: Vec<_> = mux.iter_windows_in_workspace(workspace);
        for window_id in window_ids {
            mux.kill_window(window_id);
        }
        self.sidebar.workspace_configs.remove_workspace(workspace);
    }

    /// Open a single-line prompt overlay so the user can set a custom display
    /// name (nickname) for the given workspace. An empty submission clears the
    /// override; pressing Esc leaves it unchanged.
    pub fn show_workspace_nickname_prompt(&mut self, workspace: String) {
        let mux = Mux::get();
        let tab = match mux.get_active_tab_for_window(self.mux_window_id) {
            Some(tab) => tab,
            None => return,
        };

        let initial = self
            .sidebar
            .workspace_configs
            .workspaces
            .get(&workspace)
            .and_then(|c| c.display_name.clone());

        let gui_win = GuiWin::new(self);
        let workspace_for_overlay = workspace.clone();

        let (overlay, future) = crate::overlay::start_overlay(self, &tab, move |_tab_id, term| {
            crate::overlay::prompt::show_workspace_nickname_prompt_overlay(
                term,
                workspace_for_overlay,
                initial,
                gui_win,
            )
        });
        self.assign_overlay(tab.tab_id(), overlay);
        promise::spawn::spawn(future).detach();
    }

    /// Open the workspace emoji picker modal for the given workspace.
    /// Replaces the legacy curated-preset native submenu.
    pub fn show_workspace_emoji_picker(&mut self, workspace: String) {
        use crate::termwindow::workspace_emoji_picker::WorkspaceEmojiPicker;
        let modal = WorkspaceEmojiPicker::new(workspace);
        self.set_modal(std::rc::Rc::new(modal));
    }

    /// Apply an emoji selection from the workspace emoji picker modal and
    /// dismiss it. Passing `None` clears the emoji.
    pub fn apply_workspace_emoji_from_picker(&mut self, emoji: Option<String>) {
        use crate::termwindow::workspace_emoji_picker::WorkspaceEmojiPicker;
        let workspace = {
            let modal = match self.get_modal() {
                Some(m) => m,
                None => return,
            };
            match (*modal).downcast_ref::<WorkspaceEmojiPicker>() {
                Some(picker) => picker.workspace().to_string(),
                None => return,
            }
        };
        self.sidebar.workspace_configs.set_emoji(&workspace, emoji);
        if let Err(e) = self.sidebar.workspace_configs.save() {
            log::error!("Failed to save workspace configs: {:#}", e);
        }
        self.sidebar.invalidate_cache();
        self.cancel_modal();
        if let Some(window) = self.window.as_ref() {
            window.invalidate();
        }
    }

    /// Called from the prompt overlay when the user submits a nickname value.
    /// `value` is the raw submitted string; empty/whitespace-only clears.
    pub fn apply_workspace_nickname(&mut self, workspace: String, value: String) {
        let trimmed = value.trim();
        let display_name = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
        self.sidebar
            .workspace_configs
            .set_display_name(&workspace, display_name);
        if let Err(e) = self.sidebar.workspace_configs.save() {
            log::error!("Failed to save workspace configs: {:#}", e);
        }
        self.sidebar.invalidate_cache();
        if let Some(window) = self.window.as_ref() {
            window.invalidate();
        }
    }

    pub fn show_new_workspace_prompt(&mut self) {
        let name = Mux::get().generate_workspace_name();
        // Clear any stale config from a previous workspace with the same recycled name
        self.sidebar.workspace_configs.remove_workspace(&name);
        // Explicitly place new workspace at the bottom of the ordering
        // so it doesn't jump to a random alphabetical position.
        let all = Mux::get().iter_workspaces();
        self.sidebar.workspace_configs.move_to_bottom(&name, &all);
        if let Err(e) = self.sidebar.workspace_configs.save() {
            log::error!("Failed to save workspace configs: {:#}", e);
        }
        self.spawn_named_workspace(name);
    }

    pub fn spawn_named_workspace(&mut self, name: String) {
        let activity = crate::Activity::new();
        let mux = Mux::get();
        let switcher = WorkspaceSwitcher::new(&name);
        mux.set_active_workspace(&name);

        if mux.iter_windows_in_workspace(&name).is_empty() {
            let size = self.terminal_size;
            let term_config = Arc::new(TermConfig::with_config(self.config.clone()));
            let src_window_id = self.mux_window_id;

            promise::spawn::spawn(async move {
                if let Err(err) = crate::spawn::spawn_command_internal(
                    Default::default(),
                    SpawnWhere::NewWindow,
                    size,
                    Some(src_window_id),
                    term_config,
                )
                .await
                {
                    log::error!("Failed to spawn workspace `{}`: {:#}", name, err);
                }
                switcher.do_switch();
                drop(activity);
            })
            .detach();
        } else {
            switcher.do_switch();
        }
    }

    fn maybe_refresh_sidebar_metadata(&mut self, request: &SidebarRefreshRequest) {
        if !self.sidebar.visible
            || self.sidebar.metadata_refresh_in_flight
            || (request.targets.is_empty() && request.pane_leaders.is_empty())
        {
            return;
        }

        let Some(deadline) = self.sidebar.next_metadata_refresh else {
            return;
        };
        if Instant::now() < deadline {
            return;
        }

        let request = request.clone();
        let Some(window) = self.window.as_ref().cloned() else {
            return;
        };

        self.sidebar.metadata_refresh_in_flight = true;
        self.sidebar.next_metadata_refresh = None;
        let existing_metadata = self.sidebar.metadata.clone();

        spawn(async move {
            let result = spawn_into_new_thread(move || {
                Ok(collect_sidebar_metadata(request, existing_metadata))
            })
            .await;

            match result {
                Ok(refreshed) => {
                    window.notify(crate::termwindow::TermWindowNotif::Apply(Box::new(
                        move |term_window| {
                            term_window.finish_sidebar_metadata_refresh(refreshed);
                        },
                    )));
                }
                Err(err) => {
                    log::error!("Failed to refresh sidebar metadata: {err:#}");
                    window.notify(crate::termwindow::TermWindowNotif::Apply(Box::new(
                        move |term_window| {
                            term_window.sidebar.metadata_refresh_in_flight = false;
                        },
                    )));
                }
            }
        })
        .detach();
    }

    fn finish_sidebar_metadata_refresh(&mut self, refreshed: SidebarRefreshResult) {
        let SidebarRefreshResult {
            metadata,
            pane_facts,
        } = refreshed;

        // Only move the facts generation when the facts actually changed —
        // the sidebar entry cache keys off it, and rebuilding is wasted work
        // if the answer is identical.
        if pane_facts != self.sidebar.pane_process_facts {
            self.sidebar.pane_process_facts = pane_facts;
            self.sidebar.pane_facts_generation += 1;
        }

        // Now that we know what each pane is actually running, reap agents
        // that have exited.  This is the write half of what `sidebar_entries`
        // used to do inline; doing it here keeps entry building read-only.
        self.reap_departed_agents();

        // Sync to Mux for session persistence
        if let Some(mux) = mux::Mux::try_get() {
            let cache: HashMap<String, mux::session::SidebarCacheSerde> = metadata
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        mux::session::SidebarCacheSerde {
                            git_branch: v.git_branch.clone(),
                            git_dirty: v.git_dirty,
                            listening_ports: v.listening_ports.clone(),
                            pull_request: v.pull_request.as_ref().map(|pr| {
                                mux::session::PullRequestSerde {
                                    number: pr.number,
                                    status: match pr.status {
                                        WorkspacePullRequestStatus::Open => "Open".to_string(),
                                        WorkspacePullRequestStatus::Merged => "Merged".to_string(),
                                        WorkspacePullRequestStatus::Closed => "Closed".to_string(),
                                    },
                                }
                            }),
                        },
                    )
                })
                .collect();
            mux.set_sidebar_cache(cache);
        }

        self.sidebar.metadata = metadata;
        self.sidebar.metadata_refresh_in_flight = false;
        self.invalidate_fancy_tab_bar();

        if let Some(window) = self.window.as_ref() {
            window.invalidate();
        }
    }
}

fn detect_agent_type(info: &LocalProcessInfo) -> Option<AgentType> {
    detect_agent_type_from_names(info.flatten_to_exe_names())
}

fn detect_agent_type_from_names<I, S>(exe_names: I) -> Option<AgentType>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for name in exe_names {
        let lower = name.as_ref().to_lowercase();
        if lower.contains("claude") {
            return Some(AgentType::ClaudeCode);
        }
        if lower == "codex" {
            return Some(AgentType::Codex);
        }
        if lower == "omp" {
            return Some(AgentType::Omp);
        }
        if lower.contains("cursor") {
            return Some(AgentType::Cursor);
        }
        if lower == "opencode" {
            return Some(AgentType::OpenCode);
        }
        if lower == "aider" {
            return Some(AgentType::Aider);
        }
    }
    None
}

fn agent_type_display_name(agent_type: AgentType) -> String {
    match agent_type {
        AgentType::ClaudeCode => "Claude Code".to_string(),
        AgentType::Codex => "Codex".to_string(),
        AgentType::Omp => "Oh My Pi".to_string(),
        AgentType::Cursor => "Cursor".to_string(),
        AgentType::OpenCode => "OpenCode".to_string(),
        AgentType::Aider => "Aider".to_string(),
    }
}

fn build_agent_info(
    detected_type: Option<AgentType>,
    pane_id: Option<mux::pane::PaneId>,
    cached_agent_type: Option<AgentType>,
) -> Option<AgentInfo> {
    // Check the structured status store (populated via OSC 7777)
    let pane_status = pane_id.and_then(|id| Mux::get().agent_status_for_pane(id));

    // Resolve agent type: prefer live detection, then cached type, then
    // default to ClaudeCode when OSC 7777 data exists (so the preview still
    // shows even on first transient process detection failure).
    let agent_type = detected_type.or(cached_agent_type).or_else(|| {
        if pane_status.is_some() {
            Some(AgentType::ClaudeCode)
        } else {
            None
        }
    });

    let agent_type = agent_type?;

    let subagent_count = pane_status.as_ref().map(|s| s.subagent_count).unwrap_or(0);
    let background_tasks_count = pane_status
        .as_ref()
        .map(|s| s.background_tasks_count)
        .unwrap_or(0);
    let session_crons_count = pane_status
        .as_ref()
        .map(|s| s.session_crons_count)
        .unwrap_or(0);
    let conversation_title = pane_status
        .as_ref()
        .and_then(|s| s.conversation_title.clone());

    let (status, status_message) = if let Some(pane_status) = pane_status {
        let status = match pane_status.status {
            mux::agent_status::AgentStatus::Working => AgentStatus::Working,
            mux::agent_status::AgentStatus::Idle => AgentStatus::Idle,
            mux::agent_status::AgentStatus::NeedsInput => AgentStatus::NeedsInput,
        };
        // When the agent is idle/needs_input, prefer the last working message
        // (the actual output preview) over the current message (which is often
        // a generic status like "Claude is waiting for your input").
        let tool_fallback = pane_status.tool.as_ref().map(|t| format!("Running {t}..."));
        let msg = match status {
            AgentStatus::Working => pane_status
                .message
                .or(pane_status.last_working_message)
                .or(tool_fallback),
            _ => pane_status
                .last_working_message
                .or(pane_status.message)
                .or(tool_fallback),
        };
        (status, msg)
    } else {
        // Agent detected as foreground process but no OSC 7777 data —
        // we can't tell if it's working or idle, so use Unknown (no label shown).
        (AgentStatus::Unknown, None)
    };

    Some(AgentInfo {
        display_name: agent_type_display_name(agent_type),
        agent_type,
        conversation_title,
        status,
        status_message,
        subagent_count,
        background_tasks_count,
        session_crons_count,
    })
}

fn sidebar_path(url: &Url) -> Option<PathBuf> {
    if url.scheme() == "file" {
        return url.to_file_path().ok();
    }

    None
}

fn sidebar_title_from_tab(tab: &mux::tab::Tab) -> Option<String> {
    tab.get_active_pane()
        .and_then(sidebar_title_from_pane)
        .or_else(|| {
            let title = tab.get_title();
            let trimmed = title.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
}

fn sidebar_title_from_pane(pane: Arc<dyn mux::pane::Pane>) -> Option<String> {
    let title = pane.get_title();
    let trimmed = title.trim();
    if !trimmed.is_empty() {
        return Some(trimmed.to_string());
    }

    pane.get_foreground_process_name(CachePolicy::AllowStale)
        .map(|name| {
            std::path::Path::new(&name)
                .file_name()
                .map(|part| part.to_string_lossy().to_string())
                .unwrap_or(name)
        })
}

fn sidebar_context_from_active_tab(
    tab: &mux::tab::Tab,
    facts: &HashMap<PaneId, PaneProcessFacts>,
) -> Option<(Option<String>, Option<PathBuf>)> {
    tab.get_active_pane()
        .and_then(|pane| sidebar_context_from_pane(pane, facts))
        .or_else(|| sidebar_context_from_tab(tab, facts))
}

fn sidebar_context_from_tab(
    tab: &mux::tab::Tab,
    facts: &HashMap<PaneId, PaneProcessFacts>,
) -> Option<(Option<String>, Option<PathBuf>)> {
    tab.iter_panes()
        .into_iter()
        .find_map(|positioned| sidebar_context_from_pane(positioned.pane, facts))
}

fn sidebar_context_from_pane(
    pane: Arc<dyn mux::pane::Pane>,
    facts: &HashMap<PaneId, PaneProcessFacts>,
) -> Option<(Option<String>, Option<PathBuf>)> {
    // Cheap: served from the pty leader cache, which refreshes itself in the
    // background.
    let cwd_url = pane.get_current_working_dir(CachePolicy::AllowStale);
    let cwd_label = cwd_url.as_ref().and_then(sidebar_path_label);
    let cwd_path = cwd_url.as_ref().and_then(sidebar_path);

    if cwd_label.is_some() || cwd_path.is_some() {
        return Some((cwd_label, cwd_path));
    }

    // Otherwise fall back to the foreground process's cwd as resolved by the
    // last refresh.  Deriving it here would mean walking the process table
    // during paint.
    facts
        .get(&pane.pane_id())
        .and_then(|facts| facts.cwd.clone())
        .map(|path| {
            let label = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string());
            (label, Some(path))
        })
}

fn sidebar_path_label(url: &Url) -> Option<String> {
    if url.scheme() == "file" {
        return url.to_file_path().ok().and_then(|path| {
            path.file_name()
                .or_else(|| path.components().next_back().map(|part| part.as_os_str()))
                .map(|name| name.to_string_lossy().to_string())
        });
    }

    url.path_segments()
        .and_then(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .next_back()
                .map(ToString::to_string)
        })
        .or_else(|| {
            let path = url.path().trim_matches('/');
            if path.is_empty() {
                None
            } else {
                Some(path.to_string())
            }
        })
}

fn collect_sidebar_metadata(
    request: SidebarRefreshRequest,
    existing_metadata: HashMap<String, WorkspaceMetadata>,
) -> SidebarRefreshResult {
    // Resolve every pane's foreground process first.  This is the expensive
    // part, and the reason this function runs on a worker thread rather than
    // during paint.
    let pane_facts = resolve_pane_process_facts(&request.pane_leaders);

    let mut metadata = HashMap::new();
    for target in request.targets {
        // Deduped: sibling panes can share a foreground process group, and
        // this becomes an `lsof -p` argument.
        let process_ids: Vec<u32> = target
            .pane_ids
            .iter()
            .filter_map(|pane_id| pane_facts.get(pane_id).and_then(|facts| facts.pid))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let previous = existing_metadata.get(&target.workspace_name);
        metadata.insert(
            target.workspace_name.clone(),
            load_workspace_metadata(&target.cwd_path, &process_ids, previous),
        );
    }

    SidebarRefreshResult {
        metadata,
        pane_facts,
    }
}

/// Resolve what each pane is running.
///
/// Every lookup shares one snapshot of the system process table, so this
/// costs a single walk regardless of how many panes are open.
fn resolve_pane_process_facts(pane_leaders: &[(PaneId, u32)]) -> HashMap<PaneId, PaneProcessFacts> {
    pane_leaders
        .iter()
        .map(|&(pane_id, leader_pid)| {
            let facts =
                match LocalProcessInfo::with_root_pid_cached(leader_pid, PANE_PROCESS_FACTS_TTL) {
                    Some(info) => PaneProcessFacts {
                        resolved: true,
                        pid: Some(info.pid),
                        agent_type: detect_agent_type(&info),
                        cwd: if info.cwd.as_os_str().is_empty() {
                            None
                        } else {
                            Some(info.cwd.clone())
                        },
                        foreground_process_name: tree_process_name(&info),
                    },
                    // Leader already gone — a transient failure, not evidence
                    // that whatever was running here has exited.
                    None => PaneProcessFacts::default(),
                };
            (pane_id, facts)
        })
        .collect()
}

/// Pick an executable name out of a process tree, for the card's icon.
///
/// Sorted rather than taken in hash order: `flatten_to_exe_names` returns a
/// `HashSet`, so an arbitrary pick would vary between refreshes, make the
/// facts compare unequal every time, and force a rebuild for no visible
/// reason (it would also flicker the icon).
fn tree_process_name(info: &LocalProcessInfo) -> Option<String> {
    let mut names: Vec<String> = info.flatten_to_exe_names().into_iter().collect();
    names.sort();
    names.pop().map(|name| {
        Path::new(&name)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or(name)
    })
}

fn load_workspace_metadata(
    cwd_path: &Path,
    process_ids: &[u32],
    existing: Option<&WorkspaceMetadata>,
) -> WorkspaceMetadata {
    let repo = Repository::discover(cwd_path).ok();
    let git_branch = repo.as_ref().and_then(|repo| {
        repo.head().ok().and_then(|head| {
            head.shorthand()
                .map(ToString::to_string)
                .or_else(|| head.target().map(|oid| oid.to_string()[..7].to_string()))
        })
    });
    let now = Instant::now();
    let should_refresh_pull_request = existing
        .and_then(|metadata| {
            let branch_matches =
                metadata.pull_request_checked_for_branch.as_deref() == git_branch.as_deref();
            let fresh_enough = metadata
                .pull_request_checked_at
                .map(|checked_at| {
                    now.duration_since(checked_at) < SIDEBAR_PULL_REQUEST_REFRESH_INTERVAL
                })
                .unwrap_or(false);
            if branch_matches && fresh_enough {
                Some(false)
            } else {
                Some(true)
            }
        })
        .unwrap_or(true);

    let pull_request = if should_refresh_pull_request {
        repo.as_ref().and_then(|repo| {
            repo.workdir()
                .or_else(|| repo.path().parent())
                .and_then(load_pull_request)
        })
    } else {
        existing.and_then(|metadata| metadata.pull_request.clone())
    };
    let pull_request_checked_for_branch = if should_refresh_pull_request {
        repo.as_ref().and_then(|_| git_branch.clone())
    } else {
        existing.and_then(|metadata| metadata.pull_request_checked_for_branch.clone())
    };
    let pull_request_checked_at = if should_refresh_pull_request {
        repo.as_ref().map(|_| now)
    } else {
        existing.and_then(|metadata| metadata.pull_request_checked_at)
    };

    WorkspaceMetadata {
        git_branch,
        git_dirty: repo
            .as_ref()
            .and_then(|repo| repo_has_changes(repo).ok())
            .unwrap_or(false),
        listening_ports: load_listening_ports(process_ids),
        pull_request,
        pull_request_checked_for_branch,
        pull_request_checked_at,
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

fn parse_listening_ports(output: &str) -> Vec<u16> {
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

fn load_pull_request(repo_root: &Path) -> Option<WorkspacePullRequest> {
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

fn parse_pull_request(output: &str) -> Option<WorkspacePullRequest> {
    let payload: GitHubPullRequestPayload = serde_json::from_str(output).ok()?;
    let status =
        if payload.merged_at.is_some() || matches!(payload.state, GitHubPullRequestState::Merged) {
            WorkspacePullRequestStatus::Merged
        } else {
            match payload.state {
                GitHubPullRequestState::Open => WorkspacePullRequestStatus::Open,
                GitHubPullRequestState::Closed => WorkspacePullRequestStatus::Closed,
                GitHubPullRequestState::Merged => WorkspacePullRequestStatus::Merged,
            }
        };

    Some(WorkspacePullRequest {
        number: payload.number,
        status,
    })
}

#[cfg(test)]
mod test {
    use super::{
        detect_agent_type_from_names, parse_listening_ports, parse_pull_request, AgentType,
        SidebarState, WorkspacePullRequest, WorkspacePullRequestStatus,
    };
    use std::time::Duration;

    #[test]
    fn metadata_refresh_coalesces_rather_than_postponing() {
        let mut state = SidebarState::default();

        state.schedule_metadata_refresh(Duration::from_millis(200));
        let first = state.next_metadata_refresh.expect("should be scheduled");

        // Every chunk of pane output asks for a refresh.  A later request must
        // not push the deadline back, or a continuously outputting pane starves
        // the refresh forever — which is when it matters most.
        state.schedule_metadata_refresh(Duration::from_millis(200));
        assert_eq!(state.next_metadata_refresh, Some(first));

        // An *earlier* deadline should still win.
        state.schedule_metadata_refresh(Duration::ZERO);
        assert!(state.next_metadata_refresh.expect("still scheduled") <= first);
    }

    #[test]
    fn parses_listening_ports_from_lsof_output() {
        let output = "\
p123\n\
n*:3000\n\
n127.0.0.1:5173\n\
n[::1]:8080\n\
n*:3000\n";

        assert_eq!(parse_listening_ports(output), vec![3000, 5173, 8080]);
    }

    #[test]
    fn parses_pull_request_payload() {
        let payload = r#"{"number":704,"state":"OPEN","mergedAt":null}"#;

        assert_eq!(
            parse_pull_request(payload),
            Some(WorkspacePullRequest {
                number: 704,
                status: WorkspacePullRequestStatus::Open,
            })
        );
    }

    #[test]
    fn detects_omp_as_an_agent_process() {
        assert_eq!(detect_agent_type_from_names(["omp"]), Some(AgentType::Omp));
        assert_eq!(detect_agent_type_from_names(["romp"]), None);
    }
}
