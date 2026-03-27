use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Per-workspace visual customization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceCustomization {
    /// Display name override (None = use workspace name)
    pub display_name: Option<String>,
    /// Accent color as hex string (e.g. "#ff6b6b"), None = use theme default
    pub accent_color: Option<String>,
}

/// Persistent workspace configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceConfigs {
    /// Per-workspace customizations, keyed by original workspace name
    pub workspaces: HashMap<String, WorkspaceCustomization>,
    /// Explicit workspace ordering. Workspaces not in this list appear at the end in default order.
    pub workspace_order: Vec<String>,
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

    /// Get accent color for workspace, validated as #RRGGBB.
    pub fn accent_color(&self, workspace: &str) -> Option<String> {
        self.workspaces
            .get(workspace)
            .and_then(|c| c.accent_color.as_ref())
            .filter(|color| is_valid_hex_color(color))
            .cloned()
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
        let entry = self
            .workspaces
            .entry(workspace.to_string())
            .or_default();
        entry.accent_color = color;
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

    /// Move workspace up in order (towards index 0).
    pub fn move_up(&mut self, workspace: &str, all_workspaces: &[String]) {
        self.ensure_full_order(all_workspaces);
        if let Some(pos) = self.workspace_order.iter().position(|n| n == workspace) {
            if pos > 0 {
                self.workspace_order.swap(pos, pos - 1);
            }
        }
    }

    /// Move workspace down in order (towards the end).
    pub fn move_down(&mut self, workspace: &str, all_workspaces: &[String]) {
        self.ensure_full_order(all_workspaces);
        if let Some(pos) = self.workspace_order.iter().position(|n| n == workspace) {
            if pos + 1 < self.workspace_order.len() {
                self.workspace_order.swap(pos, pos + 1);
            }
        }
    }

    /// Move workspace to top of the order.
    pub fn move_to_top(&mut self, workspace: &str, all_workspaces: &[String]) {
        self.ensure_full_order(all_workspaces);
        if let Some(pos) = self.workspace_order.iter().position(|n| n == workspace) {
            let name = self.workspace_order.remove(pos);
            self.workspace_order.insert(0, name);
        }
    }

    /// Move workspace to bottom of the order.
    pub fn move_to_bottom(&mut self, workspace: &str, all_workspaces: &[String]) {
        self.ensure_full_order(all_workspaces);
        if let Some(pos) = self.workspace_order.iter().position(|n| n == workspace) {
            let name = self.workspace_order.remove(pos);
            self.workspace_order.push(name);
        }
    }

    /// Remove workspace from configs (called when workspace is closed).
    pub fn remove_workspace(&mut self, workspace: &str) {
        self.workspaces.remove(workspace);
        self.workspace_order.retain(|n| n != workspace);
    }

    /// Ensure workspace_order contains all current workspaces.
    /// Workspaces already in the order keep their position.
    /// Missing workspaces are appended in the order they appear in all_workspaces.
    /// Stale entries (not in all_workspaces) are removed.
    fn ensure_full_order(&mut self, all_workspaces: &[String]) {
        // Remove stale entries
        self.workspace_order
            .retain(|n| all_workspaces.contains(n));

        // Append missing workspaces
        for name in all_workspaces {
            if !self.workspace_order.contains(name) {
                self.workspace_order.push(name.clone());
            }
        }
    }
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
        assert!(
            configs
                .workspaces
                .get("ws")
                .and_then(|c| c.accent_color.as_ref())
                .is_none()
        );
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
    fn test_ensure_full_order() {
        let mut configs = WorkspaceConfigs::default();
        configs.workspace_order = vec!["b".into(), "stale".into()];

        let all = vec!["a".into(), "b".into(), "c".into()];
        configs.ensure_full_order(&all);

        // "stale" removed, "a" and "c" appended in original order
        assert_eq!(configs.workspace_order, vec!["b", "a", "c"]);
    }

    #[test]
    fn test_serde_roundtrip() {
        let mut configs = WorkspaceConfigs::default();
        configs.set_display_name("project-a", Some("Project Alpha".into()));
        configs.set_accent_color("project-a", Some("#ff6b6b".into()));
        configs.workspace_order = vec!["project-a".into(), "default".into()];

        let json = serde_json::to_string_pretty(&configs).unwrap();
        let restored: WorkspaceConfigs = serde_json::from_str(&json).unwrap();

        assert_eq!(
            restored.display_name("project-a"),
            "Project Alpha"
        );
        assert_eq!(
            restored.accent_color("project-a"),
            Some("#ff6b6b".to_string())
        );
        assert_eq!(restored.workspace_order, vec!["project-a", "default"]);
    }
}
