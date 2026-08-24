use config::{ConfigHandle, DimensionContext, TermConfig};
use crate::remote::{PickerContext, RemoteSessions, WorkspaceTarget};
use mux::domain::DomainState;
use mux::Mux;
use mux::pane::{CachePolicy, PaneId};
use promise::spawn::spawn;
use std::collections::HashMap;
use std::path::PathBuf;
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
/// Floor on how often one workspace's metadata is gathered. Pane output alone
/// asks for a refresh many times a second while you type; a local pass costs a
/// git status, and a remote one costs a round trip plus a git status on that
/// host, so the two are throttled differently.
const SIDEBAR_METADATA_MIN_INTERVAL_LOCAL: Duration = Duration::from_millis(500);
const SIDEBAR_METADATA_MIN_INTERVAL_REMOTE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Default)]
pub struct SidebarState {
    pub visible: bool,
    pub hovered_workspace: Option<String>,
    pub override_width: Option<f32>,
    pub scroll_offset: f32,
    pub metadata: HashMap<String, WorkspaceMetadata>,
    metadata_targets: Vec<WorkspaceMetadataTarget>,
    pub metadata_refresh_in_flight: bool,
    pub next_metadata_refresh: Option<Instant>,
    /// When each workspace's metadata was last gathered.
    metadata_refreshed_at: HashMap<String, Instant>,
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

pub type WorkspacePullRequestStatus = mux::workspace_metadata::PullRequestStatus;
pub type WorkspacePullRequest = mux::workspace_metadata::PullRequestInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    ClaudeCode,
    Codex,
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
    pub status: AgentStatus,
    pub status_message: Option<String>,
    /// Number of subagents running within this agent session.
    pub subagent_count: u32,
}

/// A workspace to refresh, and the domain that owns its panes and so answers
/// for it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct WorkspaceMetadataTarget {
    workspace_name: String,
    domain_id: mux::domain::DomainId,
    /// Whether answering for this workspace costs a round trip.
    is_remote: bool,
}

impl WorkspaceMetadataTarget {
    fn min_interval(&self) -> Duration {
        if self.is_remote {
            SIDEBAR_METADATA_MIN_INTERVAL_REMOTE
        } else {
            SIDEBAR_METADATA_MIN_INTERVAL_LOCAL
        }
    }
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
            metadata_targets: vec![],
            metadata_refresh_in_flight: false,
            next_metadata_refresh: None,
            metadata_refreshed_at: HashMap::new(),
            cached_entries: None,
            cached_workspace_count: 0,
            cached_active_workspace: String::new(),
            cached_hovered_workspace: None,
            cached_agent_status_generation: 0,
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

    pub fn schedule_metadata_refresh(&mut self, delay: Duration) {
        self.next_metadata_refresh = Some(Instant::now() + delay);
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
        let structural_change = workspace_count != self.sidebar.cached_workspace_count
            || active_workspace != self.sidebar.cached_active_workspace
            || hovered != self.sidebar.cached_hovered_workspace
            || !cached_unread_match
            || !cached_metadata_match
            || agent_generation != self.sidebar.cached_agent_status_generation;
        if !structural_change {
            if let Some(ref cached) = self.sidebar.cached_entries {
                return cached.clone();
            }
        }

        let hovered = hovered.as_deref();
        let mut refresh_targets = vec![];

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
                let mut active_pane_process_info: Option<LocalProcessInfo> = None;
                let mut active_pane_id: Option<mux::pane::PaneId> = None;
                let mut domain_id: Option<mux::domain::DomainId> = None;

                for window_id in mux.iter_windows_in_workspace(&name) {
                    if let Some(window) = mux.get_window(window_id) {
                        tab_count += window.len();
                        if title == name {
                            if let Some(tab) = window.get_active() {
                                title = sidebar_title_from_tab(tab.as_ref())
                                    .unwrap_or_else(|| name.clone());
                                if active_pane_process_info.is_none() {
                                    if let Some(pane) = tab.get_active_pane() {
                                        active_pane_id = Some(pane.pane_id());
                                        active_pane_process_info = pane
                                            .get_foreground_process_info(CachePolicy::AllowStale);
                                    }
                                }
                            }
                        }
                        if domain_id.is_none() {
                            domain_id = window
                                .get_active()
                                .and_then(|tab| tab.get_active_pane())
                                .or_else(|| {
                                    window.iter().find_map(|tab| {
                                        tab.iter_panes().into_iter().next().map(|p| p.pane)
                                    })
                                })
                                .map(|pane| pane.domain_id());
                        }
                        if cwd.is_none() || cwd_path.is_none() {
                            if let Some((label, path)) = window
                                .get_active()
                                .and_then(|tab| sidebar_context_from_active_tab(tab.as_ref()))
                                .or_else(|| {
                                    window
                                        .iter()
                                        .find_map(|tab| sidebar_context_from_tab(tab.as_ref()))
                                })
                            {
                                cwd = label;
                                cwd_path = path;
                            }
                        }
                        for tab in window.iter() {
                            pane_count += tab.count_panes().unwrap_or(0);
                        }
                    }
                }

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

                // Clear stale agent cache before building agent info.
                // If we CAN see the foreground process and it's NOT an agent,
                // the agent has genuinely exited. If process_info is None,
                // it's a transient detection failure — keep the cache.
                if let Some(pane_id) = active_pane_id {
                    if active_pane_process_info.is_some()
                        && active_pane_process_info
                            .as_ref()
                            .and_then(detect_agent_type)
                            .is_none()
                    {
                        self.sidebar.last_known_agents.remove(&pane_id);
                        mux.remove_agent_status(pane_id);
                    }
                }

                let agent = build_agent_info(
                    active_pane_process_info.as_ref(),
                    active_pane_id,
                    active_pane_id.and_then(|id| self.sidebar.last_known_agents.get(&id).copied()),
                );

                // Cache detected agent type so transient process detection
                // failures don't wipe the preview on the next render frame.
                if let Some(pane_id) = active_pane_id {
                    if let Some(ref agent) = agent {
                        self.sidebar
                            .last_known_agents
                            .insert(pane_id, agent.agent_type);
                    }
                }

                if let Some(domain_id) = domain_id {
                    let is_remote = mux
                        .get_domain(domain_id)
                        .map(|domain| {
                            domain
                                .downcast_ref::<wezterm_client::domain::ClientDomain>()
                                .is_some()
                        })
                        .unwrap_or(false);
                    refresh_targets.push(WorkspaceMetadataTarget {
                        workspace_name: name.clone(),
                        domain_id,
                        is_remote,
                    });
                }

                // Apply display name override from workspace config
                let display_title = self.sidebar.workspace_configs.display_name(&name);
                let title = if display_title != name {
                    display_title
                } else {
                    title
                };
                let accent_color = self.sidebar.workspace_configs.accent_color(&name);
                let emoji = self.sidebar.workspace_configs.emoji(&name);

                // Extract foreground process name for icon display
                let foreground_process_name = active_pane_process_info.as_ref().and_then(|info| {
                    info.flatten_to_exe_names().into_iter().last().map(|name| {
                        std::path::Path::new(&name)
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or(name)
                    })
                });

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
                }
            })
            .collect();

        let targets_changed = refresh_targets != self.sidebar.metadata_targets;
        let missing_metadata = refresh_targets
            .iter()
            .any(|target| !self.sidebar.metadata.contains_key(&target.workspace_name));
        let mut scheduled_refresh = false;

        if targets_changed {
            self.sidebar.metadata_targets = refresh_targets.clone();
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

        self.maybe_refresh_sidebar_metadata(&refresh_targets);

        // Update cache
        self.sidebar.cached_entries = Some(entries.clone());
        self.sidebar.cached_workspace_count = workspace_count;
        self.sidebar.cached_active_workspace = active_workspace;
        self.sidebar.cached_hovered_workspace = self.sidebar.hovered_workspace.clone();
        self.sidebar.cached_agent_status_generation = agent_generation;

        entries
    }

    /// Ask for a metadata pass. Cheap: no repaint and no layout thrown away,
    /// so it is safe to call for every chunk of pane output.
    pub fn schedule_sidebar_metadata_refresh(&mut self) {
        self.sidebar
            .schedule_metadata_refresh(SIDEBAR_METADATA_COALESCE_DELAY);
    }

    /// As above, and redraw now: for the events that change what the sidebar
    /// shows rather than merely when it was last gathered.
    pub fn refresh_sidebar(&mut self) {
        self.schedule_sidebar_metadata_refresh();
        self.sidebar.invalidate_cache();
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
    ///        6=Close, 100=ColorReset, 101-108=Color swatches,
    ///        200=EmojiReset, 201+=Emoji presets
    pub fn handle_context_menu_selection(&mut self, tag: usize, window: &::window::Window) {
        let workspace = match self.sidebar.context_menu_workspace.take() {
            Some(name) => name,
            None => return,
        };

        const COLOR_HEXES: &[&str] = &[
            "#ff6b6b", "#ffa94d", "#ffd43b", "#69db7c", "#38d9a9", "#4dabf7", "#b197fc", "#f783ac",
        ];
        let emoji_presets = super::mouseevent::EMOJI_PRESETS;

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
            200 => {
                self.sidebar
                    .workspace_configs
                    .set_emoji(&workspace, None);
            }
            tag if tag >= 201 && tag < 201 + emoji_presets.len() => {
                let idx = tag - 201;
                if let Some(emoji) = emoji_presets.get(idx) {
                    self.sidebar
                        .workspace_configs
                        .set_emoji(&workspace, Some((*emoji).to_string()));
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

    /// Open the picker that chooses where a new workspace runs. With no remote
    /// host known, there is nothing to choose, so spawn locally right away.
    pub fn show_new_workspace_prompt(&mut self) {
        let config = config::configuration();
        let sessions = RemoteSessions::load();
        let ctx = self.workspace_picker_context(&config, &sessions);
        let entries = crate::remote::build_picker_entries(&sessions, &ctx);

        if entries.len() <= 1 {
            self.spawn_local_workspace();
            return;
        }

        let mux = Mux::get();
        let Some(tab) = mux.get_active_tab_for_window(self.mux_window_id) else {
            return;
        };
        let gui_win = GuiWin::new(self);

        let (overlay, future) = crate::overlay::start_overlay(self, &tab, move |_tab_id, term| {
            crate::overlay::workspace_picker::show_workspace_picker_overlay(term, entries, gui_win)
        });
        self.assign_overlay(tab.tab_id(), overlay);
        promise::spawn::spawn(future).detach();
    }

    /// Gather what the picker needs from the mux and the config.
    fn workspace_picker_context(
        &self,
        config: &ConfigHandle,
        sessions: &RemoteSessions,
    ) -> PickerContext {
        let mux = Mux::get();
        let attached_hosts = sessions
            .hosts()
            .into_iter()
            .filter(|host| {
                crate::remote::domain_config_for_host(config, host)
                    .ok()
                    .and_then(|dom| mux.get_domain_by_name(&dom.name))
                    .map(|dom| dom.state() == DomainState::Attached)
                    .unwrap_or(false)
            })
            .collect();

        PickerContext {
            live_workspaces: mux.iter_workspaces(),
            attached_hosts,
            configured_hosts: crate::remote::explicitly_configured_hosts(config),
        }
    }

    /// Prompt for a host that isn't in the picker yet.
    pub fn show_remote_host_prompt(&mut self) {
        let mux = Mux::get();
        let Some(tab) = mux.get_active_tab_for_window(self.mux_window_id) else {
            return;
        };
        let gui_win = GuiWin::new(self);

        let (overlay, future) = crate::overlay::start_overlay(self, &tab, move |_tab_id, term| {
            crate::overlay::prompt::show_remote_host_prompt_overlay(term, gui_win)
        });
        self.assign_overlay(tab.tab_id(), overlay);
        promise::spawn::spawn(future).detach();
    }

    pub fn spawn_workspace_target(&mut self, target: WorkspaceTarget) {
        match target {
            WorkspaceTarget::Local => self.spawn_local_workspace(),
            WorkspaceTarget::NewRemote { host } => self.spawn_remote_workspace(host, None),
            WorkspaceTarget::Remote { host, workspace } => {
                self.spawn_remote_workspace(host, Some(workspace))
            }
            WorkspaceTarget::PromptForHost => self.show_remote_host_prompt(),
        }
    }

    fn spawn_local_workspace(&mut self) {
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

    /// Attach `host`'s mux server if needed, then open `workspace` there, or a
    /// new one when it is `None`.
    fn spawn_remote_workspace(&mut self, host: String, workspace: Option<String>) {
        let activity = crate::Activity::new();
        let size = self.terminal_size;

        promise::spawn::spawn(async move {
            let config = config::configuration();
            let domain = match crate::remote::domain_for_host(&config, &host) {
                Ok(domain) => domain,
                Err(err) => {
                    log::error!("cannot use remote host {host}: {err:#}");
                    return;
                }
            };

            if domain.state() == DomainState::Detached {
                if let Err(err) = domain.attach(None).await {
                    log::error!("cannot attach to {host}: {err:#}");
                    return;
                }
            }

            let mux = Mux::get();
            let workspace = workspace.unwrap_or_else(|| mux.generate_workspace_name());
            let switcher = WorkspaceSwitcher::new(&workspace);
            mux.set_active_workspace(&workspace);

            if mux.iter_windows_in_workspace(&workspace).is_empty() {
                let window_id = *mux.new_empty_window(Some(workspace.clone()), None);
                if let Err(err) = domain.spawn(size, None, None, window_id).await {
                    log::error!("cannot open a workspace on {host}: {err:#}");
                }
            }
            switcher.do_switch();

            let mut sessions = RemoteSessions::load();
            sessions.record(&host, &workspace, chrono::Utc::now().to_rfc3339());
            if let Err(err) = sessions.save() {
                log::warn!("could not record the remote session: {err:#}");
            }

            drop(activity);
        })
        .detach();
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

    fn maybe_refresh_sidebar_metadata(&mut self, targets: &[WorkspaceMetadataTarget]) {
        if !self.sidebar.visible || self.sidebar.metadata_refresh_in_flight || targets.is_empty() {
            return;
        }

        let Some(deadline) = self.sidebar.next_metadata_refresh else {
            return;
        };
        if Instant::now() < deadline {
            return;
        }

        let now = Instant::now();
        let targets: Vec<_> = targets
            .iter()
            .filter(|target| {
                self.sidebar
                    .metadata_refreshed_at
                    .get(&target.workspace_name)
                    .map(|last| now.duration_since(*last) >= target.min_interval())
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        if targets.is_empty() {
            return;
        }

        let Some(window) = self.window.as_ref().cloned() else {
            return;
        };

        self.sidebar.metadata_refresh_in_flight = true;
        self.sidebar.next_metadata_refresh = None;
        let existing_metadata = self.sidebar.metadata.clone();

        spawn(async move {
            let metadata = collect_sidebar_metadata(targets, existing_metadata).await;
            window.notify(crate::termwindow::TermWindowNotif::Apply(Box::new(
                move |term_window| {
                    term_window.finish_sidebar_metadata_refresh(metadata);
                },
            )));
        })
        .detach();
    }

    fn finish_sidebar_metadata_refresh(&mut self, metadata: HashMap<String, WorkspaceMetadata>) {
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

        let now = Instant::now();
        for workspace in metadata.keys() {
            self.sidebar
                .metadata_refreshed_at
                .insert(workspace.clone(), now);
        }
        self.sidebar.metadata.extend(metadata);
        let known: Vec<String> = self.sidebar.metadata.keys().cloned().collect();
        self.sidebar
            .metadata_refreshed_at
            .retain(|workspace, _| known.contains(workspace));
        self.sidebar.metadata_refresh_in_flight = false;
        self.invalidate_fancy_tab_bar();

        if let Some(window) = self.window.as_ref() {
            window.invalidate();
        }
    }
}

fn detect_agent_type(info: &LocalProcessInfo) -> Option<AgentType> {
    let exe_names = info.flatten_to_exe_names();
    for name in &exe_names {
        let lower = name.to_lowercase();
        if lower.contains("claude") {
            return Some(AgentType::ClaudeCode);
        }
        if lower == "codex" {
            return Some(AgentType::Codex);
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
        AgentType::Cursor => "Cursor".to_string(),
        AgentType::OpenCode => "OpenCode".to_string(),
        AgentType::Aider => "Aider".to_string(),
    }
}

fn build_agent_info(
    process_info: Option<&LocalProcessInfo>,
    pane_id: Option<mux::pane::PaneId>,
    cached_agent_type: Option<AgentType>,
) -> Option<AgentInfo> {
    let detected_type = process_info.and_then(detect_agent_type);

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

    let subagent_count = pane_status
        .as_ref()
        .map(|s| s.subagent_count)
        .unwrap_or(0);

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
            AgentStatus::Working => {
                pane_status.message
                    .or(pane_status.last_working_message)
                    .or(tool_fallback)
            }
            _ => {
                pane_status.last_working_message
                    .or(pane_status.message)
                    .or(tool_fallback)
            }
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
        status,
        status_message,
        subagent_count,
    })
}

fn sidebar_path(url: &Url) -> Option<PathBuf> {
    mux::workspace_metadata::path_from_cwd_url(url)
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
) -> Option<(Option<String>, Option<PathBuf>)> {
    tab.get_active_pane()
        .and_then(sidebar_context_from_pane)
        .or_else(|| sidebar_context_from_tab(tab))
}

fn sidebar_context_from_tab(tab: &mux::tab::Tab) -> Option<(Option<String>, Option<PathBuf>)> {
    tab.iter_panes()
        .into_iter()
        .find_map(|positioned| sidebar_context_from_pane(positioned.pane))
}

fn sidebar_context_from_pane(
    pane: Arc<dyn mux::pane::Pane>,
) -> Option<(Option<String>, Option<PathBuf>)> {
    let cwd_url = pane.get_current_working_dir(CachePolicy::AllowStale);
    let cwd_label = cwd_url.as_ref().and_then(sidebar_path_label);
    let cwd_path = cwd_url.as_ref().and_then(sidebar_path);

    if cwd_label.is_some() || cwd_path.is_some() {
        return Some((cwd_label, cwd_path));
    }

    pane.get_foreground_process_info(CachePolicy::AllowStale)
        .and_then(|info| {
            if info.cwd.as_os_str().is_empty() {
                None
            } else {
                let path = info.cwd;
                let label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string());
                Some((label, Some(path)))
            }
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

/// Ask each workspace's owning domain for its metadata, merging the answer
/// with what we already knew.
async fn collect_sidebar_metadata(
    targets: Vec<WorkspaceMetadataTarget>,
    existing_metadata: HashMap<String, WorkspaceMetadata>,
) -> HashMap<String, WorkspaceMetadata> {
    let mux = Mux::get();
    let mut metadata = HashMap::new();

    for target in targets {
        let previous = existing_metadata.get(&target.workspace_name);
        let Some(domain) = mux.get_domain(target.domain_id) else {
            // Domain went away mid-refresh; keep what we had.
            if let Some(previous) = previous {
                metadata.insert(target.workspace_name.clone(), previous.clone());
            }
            continue;
        };

        let want_pull_request = wants_pull_request_refresh(previous);
        let snapshot = domain
            .workspace_metadata(&target.workspace_name, want_pull_request)
            .await;

        metadata.insert(
            target.workspace_name.clone(),
            merge_workspace_metadata(snapshot, want_pull_request, previous),
        );
    }

    metadata
}

/// Whether to re-check the pull request: never checked, the interval elapsed,
/// or the branch moved since the last check.
fn wants_pull_request_refresh(previous: Option<&WorkspaceMetadata>) -> bool {
    let Some(previous) = previous else {
        return true;
    };

    let branch_matches =
        previous.pull_request_checked_for_branch.as_deref() == previous.git_branch.as_deref();
    let fresh_enough = previous
        .pull_request_checked_at
        .map(|checked_at| checked_at.elapsed() < SIDEBAR_PULL_REQUEST_REFRESH_INTERVAL)
        .unwrap_or(false);

    !(branch_matches && fresh_enough)
}

fn merge_workspace_metadata(
    snapshot: Option<mux::workspace_metadata::WorkspaceMetadataSnapshot>,
    want_pull_request: bool,
    previous: Option<&WorkspaceMetadata>,
) -> WorkspaceMetadata {
    let Some(snapshot) = snapshot else {
        // No working directory resolved yet; keep what we had.
        return previous.cloned().unwrap_or_default();
    };

    let (pull_request, pull_request_checked_for_branch, pull_request_checked_at) =
        if want_pull_request {
            let checked_for_branch = if snapshot.is_git_repo {
                snapshot.git_branch.clone()
            } else {
                None
            };
            let checked_at = if snapshot.is_git_repo {
                Some(Instant::now())
            } else {
                None
            };
            (snapshot.pull_request, checked_for_branch, checked_at)
        } else {
            (
                previous.and_then(|m| m.pull_request.clone()),
                previous.and_then(|m| m.pull_request_checked_for_branch.clone()),
                previous.and_then(|m| m.pull_request_checked_at),
            )
        };

    WorkspaceMetadata {
        git_branch: snapshot.git_branch,
        git_dirty: snapshot.git_dirty,
        listening_ports: snapshot.listening_ports,
        pull_request,
        pull_request_checked_for_branch,
        pull_request_checked_at,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use mux::workspace_metadata::WorkspaceMetadataSnapshot;

    fn snapshot(branch: &str, pr: Option<WorkspacePullRequest>) -> WorkspaceMetadataSnapshot {
        WorkspaceMetadataSnapshot {
            git_branch: Some(branch.to_string()),
            git_dirty: false,
            listening_ports: vec![3000],
            pull_request: pr,
            is_git_repo: true,
        }
    }

    #[test]
    fn first_refresh_wants_pull_request() {
        assert!(wants_pull_request_refresh(None));
    }

    #[test]
    fn fresh_check_on_same_branch_skips_pull_request() {
        let metadata = WorkspaceMetadata {
            git_branch: Some("main".to_string()),
            pull_request_checked_for_branch: Some("main".to_string()),
            pull_request_checked_at: Some(Instant::now()),
            ..Default::default()
        };
        assert!(!wants_pull_request_refresh(Some(&metadata)));
    }

    #[test]
    fn branch_change_wants_pull_request() {
        let metadata = WorkspaceMetadata {
            git_branch: Some("feature".to_string()),
            pull_request_checked_for_branch: Some("main".to_string()),
            pull_request_checked_at: Some(Instant::now()),
            ..Default::default()
        };
        assert!(wants_pull_request_refresh(Some(&metadata)));
    }

    #[test]
    fn stale_check_wants_pull_request() {
        let metadata = WorkspaceMetadata {
            git_branch: Some("main".to_string()),
            pull_request_checked_for_branch: Some("main".to_string()),
            pull_request_checked_at: Some(
                Instant::now() - SIDEBAR_PULL_REQUEST_REFRESH_INTERVAL - Duration::from_secs(1),
            ),
            ..Default::default()
        };
        assert!(wants_pull_request_refresh(Some(&metadata)));
    }

    #[test]
    fn skipped_pull_request_carries_previous_value() {
        let pr = WorkspacePullRequest {
            number: 704,
            status: WorkspacePullRequestStatus::Open,
        };
        let checked_at = Instant::now();
        let previous = WorkspaceMetadata {
            git_branch: Some("main".to_string()),
            pull_request: Some(pr.clone()),
            pull_request_checked_for_branch: Some("main".to_string()),
            pull_request_checked_at: Some(checked_at),
            ..Default::default()
        };

        let merged = merge_workspace_metadata(Some(snapshot("main", None)), false, Some(&previous));

        assert_eq!(merged.pull_request, Some(pr));
        assert_eq!(
            merged.pull_request_checked_for_branch,
            Some("main".to_string())
        );
        assert_eq!(merged.pull_request_checked_at, Some(checked_at));
        assert_eq!(merged.listening_ports, vec![3000]);
    }

    #[test]
    fn requested_pull_request_replaces_previous_value() {
        let previous = WorkspaceMetadata {
            git_branch: Some("main".to_string()),
            pull_request: Some(WorkspacePullRequest {
                number: 704,
                status: WorkspacePullRequestStatus::Open,
            }),
            pull_request_checked_for_branch: Some("main".to_string()),
            pull_request_checked_at: Some(Instant::now()),
            ..Default::default()
        };

        let merged = merge_workspace_metadata(Some(snapshot("feature", None)), true, Some(&previous));

        assert_eq!(merged.pull_request, None);
        assert_eq!(
            merged.pull_request_checked_for_branch,
            Some("feature".to_string())
        );
    }

    #[test]
    fn missing_snapshot_keeps_previous_metadata() {
        let previous = WorkspaceMetadata {
            git_branch: Some("main".to_string()),
            git_dirty: true,
            listening_ports: vec![8080],
            ..Default::default()
        };

        let merged = merge_workspace_metadata(None, true, Some(&previous));
        assert_eq!(merged, previous);
    }

    #[test]
    fn non_repo_snapshot_does_not_record_a_pull_request_check() {
        let non_repo = WorkspaceMetadataSnapshot {
            git_branch: None,
            git_dirty: false,
            listening_ports: vec![],
            pull_request: None,
            is_git_repo: false,
        };

        let merged = merge_workspace_metadata(Some(non_repo), true, None);
        assert_eq!(merged.pull_request_checked_at, None);
        assert_eq!(merged.pull_request_checked_for_branch, None);
    }
}
