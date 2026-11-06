use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Per-workspace visual customization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceCustomization {
    /// Display name override (None = use workspace name)
    pub display_name: Option<String>,
    /// Accent color as hex string (e.g. "#ff6b6b"), None = use theme default
    pub accent_color: Option<String>,
    /// Decorative emoji shown next to the workspace title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    /// Id of the sidebar group this workspace belongs to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// A named, collapsible set of workspaces in the sidebar.
///
/// Membership lives on each workspace (`WorkspaceCustomization::group`).  A
/// group's members sit together in `workspace_order`, and the group shows
/// where its first member would.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceGroup {
    /// Stable id, independent of the name, so renaming keeps membership.
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    /// `#RRGGBB`, drawn as the group's swatch and rail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Colors handed to new groups in turn, so neighbors differ by default.
pub const GROUP_COLORS: &[&str] = &[
    "#4dabf7", "#69db7c", "#b197fc", "#ffa94d", "#f783ac", "#38d9a9", "#ffd43b", "#ff6b6b",
];

/// One top-level row of the sidebar: a group with its members, or a
/// workspace outside any group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarSlot {
    Group { id: String, members: Vec<String> },
    Workspace(String),
}

impl SidebarSlot {
    fn names(&self) -> Vec<String> {
        match self {
            SidebarSlot::Group { members, .. } => members.clone(),
            SidebarSlot::Workspace(name) => vec![name.clone()],
        }
    }

    fn is_group(&self, id: &str) -> bool {
        matches!(self, SidebarSlot::Group { id: slot_id, .. } if slot_id == id)
    }

    fn is_workspace(&self, name: &str) -> bool {
        matches!(self, SidebarSlot::Workspace(slot) if slot == name)
    }
}

/// Maximum byte length we accept for an emoji. Most emoji clusters fit in 16
/// bytes; we cap at 32 to leave room for ZWJ sequences and skin-tone modifiers
/// while rejecting anything that's clearly free-form text.
const MAX_EMOJI_LEN: usize = 32;

/// Sanitize an emoji string. Trims whitespace and rejects empty or oversize input.
fn sanitize_emoji(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_EMOJI_LEN {
        return None;
    }
    Some(trimmed.to_string())
}

/// Persistent workspace configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceConfigs {
    /// Per-workspace customizations, keyed by original workspace name
    #[serde(default)]
    pub workspaces: HashMap<String, WorkspaceCustomization>,
    /// Explicit workspace ordering. Workspaces not in this list appear at the end in default order.
    #[serde(default)]
    pub workspace_order: Vec<String>,
    /// Sidebar groups, in creation order (display order follows members).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<WorkspaceGroup>,
}

/// Validate that a string is a valid #RRGGBB hex color.
fn is_valid_hex_color(s: &str) -> bool {
    if s.len() != 7 {
        return false;
    }
    let mut chars = s.chars();
    if chars.next() != Some('#') {
        return false;
    }
    chars.all(|c| c.is_ascii_hexdigit())
}

impl WorkspaceConfigs {
    /// Load from ~/.config/wezmux/workspaces.json, returns default if file doesn't exist
    /// or if the file cannot be parsed.
    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(json) => match serde_json::from_str(&json) {
                Ok(configs) => configs,
                Err(err) => {
                    log::warn!(
                        "Failed to parse {}: {:#}. Using defaults.",
                        path.display(),
                        err
                    );
                    Self::default()
                }
            },
            Err(err) => {
                log::warn!(
                    "Failed to read {}: {:#}. Using defaults.",
                    path.display(),
                    err
                );
                Self::default()
            }
        }
    }

    /// Save to ~/.config/wezmux/workspaces.json (atomic write via temp file).
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path();
        let dir = path
            .parent()
            .expect("config_path always has a parent directory");

        std::fs::create_dir_all(dir)?;

        let tmp_path = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&tmp_path, json.as_bytes())?;
        std::fs::rename(&tmp_path, &path)?;

        log::debug!("Workspace configs saved to {}", path.display());
        Ok(())
    }

    /// Get config file path: ~/.config/wezmux/workspaces.json
    fn config_path() -> PathBuf {
        config::HOME_DIR
            .join(".config")
            .join("wezmux")
            .join("workspaces.json")
    }

    /// Get display name for workspace (custom or original).
    pub fn display_name(&self, workspace: &str) -> String {
        self.workspaces
            .get(workspace)
            .and_then(|c| c.display_name.clone())
            .unwrap_or_else(|| workspace.to_string())
    }

    /// Set display name override for a workspace. Empty/whitespace-only input clears the override.
    pub fn set_display_name(&mut self, workspace: &str, display_name: Option<String>) {
        let cleaned = display_name.and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        let entry = self.workspaces.entry(workspace.to_string()).or_default();
        entry.display_name = cleaned;
        self.prune_empty_workspace(workspace);
    }

    /// Get accent color for workspace, validated as #RRGGBB.
    pub fn accent_color(&self, workspace: &str) -> Option<String> {
        self.workspaces
            .get(workspace)
            .and_then(|c| c.accent_color.as_ref())
            .filter(|color| is_valid_hex_color(color))
            .cloned()
    }

    /// Get emoji for workspace, if any.
    pub fn emoji(&self, workspace: &str) -> Option<String> {
        self.workspaces.get(workspace).and_then(|c| c.emoji.clone())
    }

    /// Set emoji for workspace. Empty/whitespace-only or oversize strings are treated as None.
    pub fn set_emoji(&mut self, workspace: &str, emoji: Option<String>) {
        let cleaned = match emoji {
            Some(e) => match sanitize_emoji(&e) {
                Some(v) => Some(v),
                None => {
                    // Treat invalid input as a clear so the call still has an effect
                    None
                }
            },
            None => None,
        };
        let entry = self.workspaces.entry(workspace.to_string()).or_default();
        entry.emoji = cleaned;
        self.prune_empty_workspace(workspace);
    }

    /// Set accent color for workspace. Validates #RRGGBB format; invalid values are ignored.
    pub fn set_accent_color(&mut self, workspace: &str, color: Option<String>) {
        if let Some(ref c) = color {
            if !is_valid_hex_color(c) {
                log::warn!(
                    "Invalid accent color '{}' for workspace '{}': expected #RRGGBB format",
                    c,
                    workspace
                );
                return;
            }
        }
        let entry = self.workspaces.entry(workspace.to_string()).or_default();
        entry.accent_color = color;
        self.prune_empty_workspace(workspace);
    }

    /// Get ordered workspace list. Takes the raw workspace list from mux, returns reordered.
    /// Workspaces in workspace_order come first (in that order), remaining appended in original order.
    pub fn apply_order(&self, workspaces: &[String]) -> Vec<String> {
        let mut result = Vec::with_capacity(workspaces.len());

        // First: workspaces from workspace_order that still exist in the live set
        for name in &self.workspace_order {
            if workspaces.contains(name) && !result.contains(name) {
                result.push(name.clone());
            }
        }

        // Then: remaining workspaces in their original order
        for name in workspaces {
            if !result.contains(name) {
                result.push(name.clone());
            }
        }

        result
    }

    /// The sidebar's top-level rows for the live `workspaces`: each group
    /// with its members together, where its first member falls in the
    /// order, and every other workspace on its own.
    pub fn layout(&self, workspaces: &[String]) -> Vec<SidebarSlot> {
        let ordered = self.apply_order(workspaces);
        let mut slots = vec![];
        let mut seen = HashSet::new();
        for name in &ordered {
            match self.group_id(name) {
                Some(id) => {
                    if seen.insert(id.to_string()) {
                        let members = ordered
                            .iter()
                            .filter(|n| self.group_id(n) == Some(id))
                            .cloned()
                            .collect();
                        slots.push(SidebarSlot::Group {
                            id: id.to_string(),
                            members,
                        });
                    }
                }
                None => slots.push(SidebarSlot::Workspace(name.clone())),
            }
        }
        slots
    }

    /// Workspaces in the order the sidebar shows them, groups included.
    /// Index-based shortcuts and "next workspace" must use this.
    pub fn display_order(&self, workspaces: &[String]) -> Vec<String> {
        flatten(&self.layout(workspaces))
    }

    pub fn group(&self, id: &str) -> Option<&WorkspaceGroup> {
        self.groups.iter().find(|g| g.id == id)
    }

    fn group_mut(&mut self, id: &str) -> Option<&mut WorkspaceGroup> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// The id of the group `workspace` belongs to, if that group exists.
    pub fn group_id(&self, workspace: &str) -> Option<&str> {
        self.workspaces
            .get(workspace)
            .and_then(|c| c.group.as_deref())
            .filter(|id| self.group(id).is_some())
    }

    /// Create an empty group and return its id.  Empty or whitespace-only
    /// names are refused.  The group is dropped again unless a workspace
    /// joins it (see `set_workspace_group`).
    pub fn create_group(&mut self, name: &str) -> Option<String> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let id = (1..)
            .map(|n| format!("g{n}"))
            .find(|id| self.group(id).is_none())
            .expect("an unused id exists");
        let color = GROUP_COLORS
            .iter()
            .find(|c| !self.groups.iter().any(|g| g.color.as_deref() == Some(**c)))
            .unwrap_or(&GROUP_COLORS[self.groups.len() % GROUP_COLORS.len()]);
        self.groups.push(WorkspaceGroup {
            id: id.clone(),
            name: name.to_string(),
            collapsed: false,
            color: Some(color.to_string()),
        });
        Some(id)
    }

    /// Put `workspace` in `group` (or in none), as the group's last member;
    /// leaving a group places it just below that group.
    pub fn set_workspace_group(
        &mut self,
        workspace: &str,
        group: Option<&str>,
        all_workspaces: &[String],
    ) {
        let group = group.filter(|id| self.group(id).is_some());
        let current = self.group_id(workspace).map(str::to_string);
        if current.as_deref() == group {
            return;
        }
        let mut all = all_workspaces.to_vec();
        if !all.iter().any(|n| n == workspace) {
            all.push(workspace.to_string());
        }
        let mut order = self.display_order(&all);
        let old_pos = order
            .iter()
            .position(|n| n == workspace)
            .unwrap_or(order.len());
        order.retain(|n| n != workspace);
        let anchor = group.or(current.as_deref());
        let pos = anchor
            .and_then(|id| {
                order
                    .iter()
                    .rposition(|n| self.group_id(n) == Some(id))
                    .map(|p| p + 1)
            })
            .unwrap_or(old_pos.min(order.len()));
        order.insert(pos, workspace.to_string());

        self.workspaces
            .entry(workspace.to_string())
            .or_default()
            .group = group.map(str::to_string);
        self.prune_empty_workspace(workspace);
        self.workspace_order = order;
        self.workspace_order = self.display_order(&all);
        self.prune_empty_groups();
    }

    /// Rename a group.  Empty or whitespace-only names are ignored.
    pub fn rename_group(&mut self, id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if let Some(group) = self.group_mut(id) {
            group.name = name.to_string();
        }
    }

    pub fn toggle_group_collapsed(&mut self, id: &str) {
        if let Some(group) = self.group_mut(id) {
            group.collapsed = !group.collapsed;
        }
    }

    /// Set a group's color; anything but `#RRGGBB` is ignored.
    pub fn set_group_color(&mut self, id: &str, color: Option<String>) {
        if color.as_deref().is_some_and(|c| !is_valid_hex_color(c)) {
            return;
        }
        if let Some(group) = self.group_mut(id) {
            group.color = color;
        }
    }

    /// Delete a group; its members stay where they are, outside any group.
    pub fn ungroup(&mut self, id: &str) {
        self.groups.retain(|g| g.id != id);
        let members: Vec<String> = self
            .workspaces
            .iter()
            .filter(|(_, c)| c.group.as_deref() == Some(id))
            .map(|(name, _)| name.clone())
            .collect();
        for name in members {
            if let Some(entry) = self.workspaces.get_mut(&name) {
                entry.group = None;
            }
            self.prune_empty_workspace(&name);
        }
    }

    /// Move a group up (`delta` < 0) or down among the top-level rows.
    pub fn move_group(&mut self, id: &str, delta: isize, all_workspaces: &[String]) {
        self.reorder(all_workspaces, |slots| {
            if let Some(pos) = slots.iter().position(|s| s.is_group(id)) {
                shift(slots, pos, pos as isize + delta);
            }
        });
    }

    /// Apply `f` to the top-level rows and store the resulting order.
    fn reorder(&mut self, all_workspaces: &[String], f: impl FnOnce(&mut Vec<SidebarSlot>)) {
        let mut slots = self.layout(all_workspaces);
        f(&mut slots);
        self.workspace_order = flatten(&slots);
    }

    /// Move `workspace` to `target(pos, len)`: within its group if it has
    /// one, otherwise among the top-level rows.
    fn move_workspace(
        &mut self,
        workspace: &str,
        all_workspaces: &[String],
        target: impl Fn(usize, usize) -> isize,
    ) {
        let group = self.group_id(workspace).map(str::to_string);
        self.reorder(all_workspaces, |slots| match group {
            Some(id) => {
                for slot in slots.iter_mut() {
                    if let SidebarSlot::Group {
                        id: slot_id,
                        members,
                    } = slot
                    {
                        if *slot_id == id {
                            if let Some(pos) = members.iter().position(|n| n == workspace) {
                                let to = target(pos, members.len());
                                shift(members, pos, to);
                            }
                        }
                    }
                }
            }
            None => {
                if let Some(pos) = slots.iter().position(|s| s.is_workspace(workspace)) {
                    let to = target(pos, slots.len());
                    shift(slots, pos, to);
                }
            }
        });
    }

    /// Move workspace up in order (towards index 0), within its group.
    pub fn move_up(&mut self, workspace: &str, all_workspaces: &[String]) {
        self.move_workspace(workspace, all_workspaces, |pos, _| pos as isize - 1);
    }

    /// Move workspace down in order (towards the end), within its group.
    pub fn move_down(&mut self, workspace: &str, all_workspaces: &[String]) {
        self.move_workspace(workspace, all_workspaces, |pos, _| pos as isize + 1);
    }

    /// Move workspace to the top of its group, or of the sidebar.
    pub fn move_to_top(&mut self, workspace: &str, all_workspaces: &[String]) {
        self.move_workspace(workspace, all_workspaces, |_, _| 0);
    }

    /// Move workspace to the bottom of its group, or of the sidebar.
    pub fn move_to_bottom(&mut self, workspace: &str, all_workspaces: &[String]) {
        let mut all = all_workspaces.to_vec();
        if !all.iter().any(|n| n == workspace) {
            // A workspace about to be spawned isn't live yet.
            all.push(workspace.to_string());
        }
        self.move_workspace(workspace, &all, |_, len| len as isize - 1);
    }

    /// Remove workspace from configs (called when workspace is closed).
    pub fn remove_workspace(&mut self, workspace: &str) {
        self.workspaces.remove(workspace);
        self.workspace_order.retain(|n| n != workspace);
        self.prune_empty_groups();
    }

    /// Drop groups no workspace belongs to.
    fn prune_empty_groups(&mut self) {
        let used: HashSet<String> = self
            .workspaces
            .values()
            .filter_map(|c| c.group.clone())
            .collect();
        self.groups.retain(|g| used.contains(&g.id));
    }

    fn prune_empty_workspace(&mut self, workspace: &str) {
        let remove = self.workspaces.get(workspace).is_some_and(|entry| {
            entry.display_name.is_none()
                && entry.accent_color.is_none()
                && entry.emoji.is_none()
                && entry.group.is_none()
        });
        if remove {
            self.workspaces.remove(workspace);
        }
    }
}

fn flatten(slots: &[SidebarSlot]) -> Vec<String> {
    slots.iter().flat_map(SidebarSlot::names).collect()
}

/// Move `v[from]` to index `to`, clamped to the vector.
fn shift<T>(v: &mut Vec<T>, from: usize, to: isize) {
    if from >= v.len() {
        return;
    }
    let to = to.clamp(0, v.len() as isize - 1) as usize;
    let item = v.remove(from);
    v.insert(to, item);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_color_validation() {
        assert!(is_valid_hex_color("#ff6b6b"));
        assert!(is_valid_hex_color("#000000"));
        assert!(is_valid_hex_color("#FFFFFF"));
        assert!(is_valid_hex_color("#aAbBcC"));

        assert!(!is_valid_hex_color("ff6b6b"));
        assert!(!is_valid_hex_color("#fff"));
        assert!(!is_valid_hex_color("#gggggg"));
        assert!(!is_valid_hex_color(""));
        assert!(!is_valid_hex_color("#ff6b6b00"));
    }

    #[test]
    fn test_display_name_default() {
        let configs = WorkspaceConfigs::default();
        assert_eq!(configs.display_name("myworkspace"), "myworkspace");
    }

    #[test]
    fn test_display_name_custom() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_display_name("myworkspace", Some("My Workspace".to_string()));
        assert_eq!(configs.display_name("myworkspace"), "My Workspace");
    }

    #[test]
    fn test_display_name_cleared() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_display_name("myworkspace", Some("Custom".to_string()));
        configs.set_display_name("myworkspace", None);
        assert_eq!(configs.display_name("myworkspace"), "myworkspace");
    }

    #[test]
    fn test_accent_color_valid() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_accent_color("ws", Some("#ff6b6b".to_string()));
        assert_eq!(configs.accent_color("ws"), Some("#ff6b6b".to_string()));
    }

    #[test]
    fn test_accent_color_invalid_rejected() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_accent_color("ws", Some("red".to_string()));
        assert_eq!(configs.accent_color("ws"), None);
        // The entry should not have been set at all
        assert!(configs
            .workspaces
            .get("ws")
            .and_then(|c| c.accent_color.as_ref())
            .is_none());
    }

    #[test]
    fn test_accent_color_clear() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_accent_color("ws", Some("#aabbcc".to_string()));
        configs.set_accent_color("ws", None);
        assert_eq!(configs.accent_color("ws"), None);
    }

    #[test]
    fn test_apply_order_basic() {
        let mut configs = WorkspaceConfigs::default();
        configs.workspace_order = vec!["b".into(), "a".into()];

        let workspaces = vec!["a".into(), "b".into(), "c".into()];
        let ordered = configs.apply_order(&workspaces);
        assert_eq!(ordered, vec!["b", "a", "c"]);
    }

    #[test]
    fn test_apply_order_stale_entries_ignored() {
        let mut configs = WorkspaceConfigs::default();
        configs.workspace_order = vec!["gone".into(), "a".into()];

        let workspaces = vec!["a".into(), "b".into()];
        let ordered = configs.apply_order(&workspaces);
        assert_eq!(ordered, vec!["a", "b"]);
    }

    #[test]
    fn test_apply_order_empty() {
        let configs = WorkspaceConfigs::default();
        let workspaces = vec!["a".into(), "b".into()];
        let ordered = configs.apply_order(&workspaces);
        assert_eq!(ordered, vec!["a", "b"]);
    }

    #[test]
    fn test_move_up() {
        let mut configs = WorkspaceConfigs::default();
        let all = vec!["a".into(), "b".into(), "c".into()];
        configs.move_up("b", &all);
        assert_eq!(configs.workspace_order, vec!["b", "a", "c"]);
    }

    #[test]
    fn test_move_up_already_at_top() {
        let mut configs = WorkspaceConfigs::default();
        let all = vec!["a".into(), "b".into(), "c".into()];
        configs.move_up("a", &all);
        assert_eq!(configs.workspace_order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_move_down() {
        let mut configs = WorkspaceConfigs::default();
        let all = vec!["a".into(), "b".into(), "c".into()];
        configs.move_down("a", &all);
        assert_eq!(configs.workspace_order, vec!["b", "a", "c"]);
    }

    #[test]
    fn test_move_down_already_at_bottom() {
        let mut configs = WorkspaceConfigs::default();
        let all = vec!["a".into(), "b".into(), "c".into()];
        configs.move_down("c", &all);
        assert_eq!(configs.workspace_order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_move_to_top() {
        let mut configs = WorkspaceConfigs::default();
        let all = vec!["a".into(), "b".into(), "c".into()];
        configs.move_to_top("c", &all);
        assert_eq!(configs.workspace_order, vec!["c", "a", "b"]);
    }

    #[test]
    fn test_move_to_bottom() {
        let mut configs = WorkspaceConfigs::default();
        let all = vec!["a".into(), "b".into(), "c".into()];
        configs.move_to_bottom("a", &all);
        assert_eq!(configs.workspace_order, vec!["b", "c", "a"]);
    }

    #[test]
    fn test_remove_workspace() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_display_name("ws1", Some("WS One".into()));
        configs.set_accent_color("ws1", Some("#aabbcc".into()));
        configs.workspace_order = vec!["ws1".into(), "ws2".into()];

        configs.remove_workspace("ws1");

        assert!(!configs.workspaces.contains_key("ws1"));
        assert_eq!(configs.workspace_order, vec!["ws2"]);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_display_name("project-a", Some("Project Alpha".into()));
        configs.set_accent_color("project-a", Some("#ff6b6b".into()));
        configs.set_emoji("project-a", Some("🚀".into()));
        configs.workspace_order = vec!["project-a".into(), "default".into()];

        let json = serde_json::to_string_pretty(&configs).unwrap();
        let restored: WorkspaceConfigs = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.display_name("project-a"), "Project Alpha");
        assert_eq!(
            restored.accent_color("project-a"),
            Some("#ff6b6b".to_string())
        );
        assert_eq!(restored.emoji("project-a"), Some("🚀".to_string()));
        assert_eq!(restored.workspace_order, vec!["project-a", "default"]);
    }

    #[test]
    fn test_emoji_set_and_get() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_emoji("ws", Some("🚀".into()));
        assert_eq!(configs.emoji("ws"), Some("🚀".to_string()));
    }

    #[test]
    fn test_emoji_default_none() {
        let configs = WorkspaceConfigs::default();
        assert_eq!(configs.emoji("ws"), None);
    }

    #[test]
    fn test_emoji_clear() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_emoji("ws", Some("🔥".into()));
        configs.set_emoji("ws", None);
        assert_eq!(configs.emoji("ws"), None);
    }

    #[test]
    fn test_emoji_empty_treated_as_clear() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_emoji("ws", Some("🔥".into()));
        configs.set_emoji("ws", Some("   ".into()));
        assert_eq!(configs.emoji("ws"), None);
    }

    #[test]
    fn test_emoji_oversize_rejected() {
        let mut configs = WorkspaceConfigs::default();
        let too_long = "x".repeat(MAX_EMOJI_LEN + 1);
        configs.set_emoji("ws", Some(too_long));
        assert_eq!(configs.emoji("ws"), None);
    }

    #[test]
    fn test_emoji_trims_whitespace() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_emoji("ws", Some("  🎯  ".into()));
        assert_eq!(configs.emoji("ws"), Some("🎯".to_string()));
    }

    #[test]
    fn test_prune_when_only_emoji_cleared() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_emoji("ws", Some("🔥".into()));
        assert!(configs.workspaces.contains_key("ws"));
        configs.set_emoji("ws", None);
        assert!(!configs.workspaces.contains_key("ws"));
    }

    #[test]
    fn test_prune_keeps_entry_when_emoji_set() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_emoji("ws", Some("🔥".into()));
        configs.set_accent_color("ws", Some("#aabbcc".into()));
        configs.set_accent_color("ws", None);
        // Should still exist because emoji is set
        assert!(configs.workspaces.contains_key("ws"));
        assert_eq!(configs.emoji("ws"), Some("🔥".to_string()));
    }

    #[test]
    fn test_display_name_trims() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_display_name("ws", Some("  Hello  ".into()));
        assert_eq!(configs.display_name("ws"), "Hello");
    }

    #[test]
    fn test_display_name_empty_input_clears() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_display_name("ws", Some("Custom".into()));
        configs.set_display_name("ws", Some("   ".into()));
        assert_eq!(configs.display_name("ws"), "ws");
    }

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// a, b, c, d, e with b and d in group "work".
    fn grouped() -> (WorkspaceConfigs, Vec<String>, String) {
        let mut configs = WorkspaceConfigs::default();
        let all = names(&["a", "b", "c", "d", "e"]);
        let id = configs.create_group("work").unwrap();
        configs.set_workspace_group("b", Some(&id), &all);
        configs.set_workspace_group("d", Some(&id), &all);
        (configs, all, id)
    }

    #[test]
    fn group_members_sit_together_at_the_first_members_place() {
        let (configs, all, id) = grouped();
        assert_eq!(
            configs.layout(&all),
            vec![
                SidebarSlot::Workspace("a".into()),
                SidebarSlot::Group {
                    id,
                    members: names(&["b", "d"]),
                },
                SidebarSlot::Workspace("c".into()),
                SidebarSlot::Workspace("e".into()),
            ]
        );
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "b", "d", "c", "e"])
        );
    }

    #[test]
    fn joining_a_group_appends_to_it() {
        let (mut configs, all, id) = grouped();
        configs.set_workspace_group("e", Some(&id), &all);
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "b", "d", "e", "c"])
        );
    }

    #[test]
    fn leaving_a_group_lands_just_below_it() {
        let (mut configs, all, _) = grouped();
        configs.set_workspace_group("b", None, &all);
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "d", "b", "c", "e"])
        );
        assert_eq!(configs.group_id("b"), None);
    }

    #[test]
    fn a_group_disappears_with_its_last_member() {
        let (mut configs, all, id) = grouped();
        configs.set_workspace_group("b", None, &all);
        configs.remove_workspace("d");
        assert!(configs.group(&id).is_none());
        assert!(configs.groups.is_empty());
    }

    #[test]
    fn an_unused_new_group_is_dropped() {
        let mut configs = WorkspaceConfigs::default();
        let all = names(&["a"]);
        let id = configs.create_group("solo").unwrap();
        configs.set_workspace_group("a", Some(&id), &all);
        let other = configs.create_group("other").unwrap();
        configs.set_workspace_group("a", Some(&other), &all);
        assert!(configs.group(&id).is_none());
        assert!(configs.group(&other).is_some());
    }

    #[test]
    fn blank_group_names_are_refused() {
        let (mut configs, _, id) = grouped();
        assert_eq!(configs.create_group("   "), None);
        configs.rename_group(&id, "  ");
        assert_eq!(configs.group(&id).unwrap().name, "work");
        configs.rename_group(&id, " wezmux ");
        assert_eq!(configs.group(&id).unwrap().name, "wezmux");
    }

    #[test]
    fn new_groups_get_distinct_colors() {
        let mut configs = WorkspaceConfigs::default();
        let a = configs.create_group("a").unwrap();
        let b = configs.create_group("b").unwrap();
        assert_ne!(
            configs.group(&a).unwrap().color,
            configs.group(&b).unwrap().color
        );
    }

    #[test]
    fn moving_a_member_stays_inside_its_group() {
        let (mut configs, all, _) = grouped();
        configs.move_up("b", &all);
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "b", "d", "c", "e"])
        );
        configs.move_down("b", &all);
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "d", "b", "c", "e"])
        );
        configs.move_to_bottom("d", &all);
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "b", "d", "c", "e"])
        );
    }

    #[test]
    fn a_loose_workspace_steps_over_a_whole_group() {
        let (mut configs, all, _) = grouped();
        configs.move_up("c", &all);
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "c", "b", "d", "e"])
        );
    }

    #[test]
    fn moving_a_group_moves_its_members() {
        let (mut configs, all, id) = grouped();
        configs.move_group(&id, 1, &all);
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "c", "b", "d", "e"])
        );
        configs.move_group(&id, -5, &all);
        assert_eq!(
            configs.display_order(&all),
            names(&["b", "d", "a", "c", "e"])
        );
    }

    #[test]
    fn ungrouping_keeps_the_order() {
        let (mut configs, all, id) = grouped();
        configs.ungroup(&id);
        assert!(configs.groups.is_empty());
        assert_eq!(configs.group_id("b"), None);
        assert_eq!(
            configs.display_order(&all),
            names(&["a", "b", "d", "c", "e"])
        );
    }

    #[test]
    fn collapsing_and_color_round_trip_through_json() {
        let (mut configs, all, id) = grouped();
        configs.toggle_group_collapsed(&id);
        configs.set_group_color(&id, Some("#123456".into()));
        configs.set_group_color(&id, Some("blue".into()));
        let json = serde_json::to_string(&configs).unwrap();
        let restored: WorkspaceConfigs = serde_json::from_str(&json).unwrap();
        let group = restored.group(&id).unwrap();
        assert!(group.collapsed);
        assert_eq!(group.color.as_deref(), Some("#123456"));
        assert_eq!(restored.display_order(&all), configs.display_order(&all));
    }

    #[test]
    fn files_from_before_groups_still_load() {
        let restored: WorkspaceConfigs =
            serde_json::from_str(r#"{"workspaces":{},"workspace_order":["a"]}"#).unwrap();
        assert!(restored.groups.is_empty());
        assert_eq!(restored.workspace_order, names(&["a"]));
    }

    #[test]
    fn a_workspace_not_yet_spawned_can_join_a_group() {
        let (mut configs, all, id) = grouped();
        configs.set_workspace_group("new", Some(&id), &all);
        let mut live = all.clone();
        live.push("new".into());
        assert_eq!(
            configs.display_order(&live),
            names(&["a", "b", "d", "new", "c", "e"])
        );
    }
}
