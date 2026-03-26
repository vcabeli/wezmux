use crate::pane::PaneId;
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Working,
    Idle,
    NeedsInput,
}

#[derive(Debug, Clone)]
pub struct AgentPaneStatus {
    pub status: AgentStatus,
    pub message: Option<String>,
    pub tool: Option<String>,
    /// The last message received while the agent was in Working state.
    /// Preserved across status transitions so the sidebar keeps showing
    /// useful output even after the agent goes idle.
    pub last_working_message: Option<String>,
    pub updated: Instant,
}

#[derive(Debug, Default)]
pub struct AgentStatusStore {
    statuses: HashMap<PaneId, AgentPaneStatus>,
    generation: u64,
}

impl AgentStatusStore {
    pub fn update_status(&mut self, pane_id: PaneId, status: AgentStatus) {
        let entry = self
            .statuses
            .entry(pane_id)
            .or_insert_with(|| AgentPaneStatus {
                status: status.clone(),
                message: None,
                tool: None,
                last_working_message: None,
                updated: Instant::now(),
            });
        entry.status = status;
        entry.updated = Instant::now();
        self.generation += 1;
    }

    pub fn update_message(&mut self, pane_id: PaneId, message: String) {
        let entry = self
            .statuses
            .entry(pane_id)
            .or_insert_with(|| AgentPaneStatus {
                status: AgentStatus::Working,
                message: None,
                tool: None,
                last_working_message: None,
                updated: Instant::now(),
            });
        // Preserve the previous message as last_working_message.
        // This way, when a generic "waiting for input" message replaces
        // a useful output, the useful output is still available.
        if let Some(ref prev) = entry.message {
            entry.last_working_message = Some(prev.clone());
        }
        entry.message = Some(message);
        entry.updated = Instant::now();
        self.generation += 1;
    }

    pub fn update_tool(&mut self, pane_id: PaneId, tool: String) {
        if let Some(entry) = self.statuses.get_mut(&pane_id) {
            entry.tool = Some(tool);
            entry.updated = Instant::now();
            self.generation += 1;
        }
    }

    pub fn clear(&mut self, pane_id: PaneId) {
        // Don't remove — preserve last known message/status so the sidebar
        // keeps showing agent info as long as the process is alive.
        // Only reset status to Idle; keep message and tool for context.
        if let Some(entry) = self.statuses.get_mut(&pane_id) {
            entry.status = AgentStatus::Idle;
            entry.updated = Instant::now();
        }
        self.generation += 1;
    }

    pub fn remove(&mut self, pane_id: PaneId) {
        self.statuses.remove(&pane_id);
        self.generation += 1;
    }

    pub fn get(&self, pane_id: PaneId) -> Option<&AgentPaneStatus> {
        self.statuses.get(&pane_id)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn update_and_get_status() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);

        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Working);
        assert!(status.message.is_none());
        assert!(status.tool.is_none());
    }

    #[test]
    fn update_message_and_tool() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "Refactoring auth".to_string());
        store.update_tool(1, "Edit".to_string());

        let status = store.get(1).unwrap();
        assert_eq!(status.message.as_deref(), Some("Refactoring auth"));
        assert_eq!(status.tool.as_deref(), Some("Edit"));
    }

    #[test]
    fn status_transition_preserves_tool() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_tool(1, "Bash".to_string());
        store.update_status(1, AgentStatus::Idle);

        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Idle);
        assert_eq!(status.tool.as_deref(), Some("Bash"));
    }

    #[test]
    fn clear_resets_to_idle() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_tool(1, "Bash".to_string());
        store.clear(1);
        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Idle);
        // Tool context is preserved for sidebar display
        assert_eq!(status.tool.as_deref(), Some("Bash"));
    }

    #[test]
    fn remove_deletes_entry() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.remove(1);
        assert!(store.get(1).is_none());
    }

    #[test]
    fn message_creates_entry_if_missing() {
        let mut store = AgentStatusStore::default();
        store.update_message(1, "orphan".to_string());
        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Working);
        assert_eq!(status.message.as_deref(), Some("orphan"));
        // First message — no previous to save
        assert!(status.last_working_message.is_none());
    }

    #[test]
    fn last_working_message_persists_across_status_transition() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "Refactoring auth module".to_string());

        // Transition to idle, then a useful final output arrives, then generic message
        store.update_status(1, AgentStatus::Idle);
        store.update_message(1, "**Still stuck.** Old process hung".to_string());
        store.update_message(1, "Claude is waiting for your input".to_string());

        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Idle);
        // Current message is the generic one
        assert_eq!(status.message.as_deref(), Some("Claude is waiting for your input"));
        // last_working_message preserves the useful output (previous message)
        assert_eq!(status.last_working_message.as_deref(), Some("**Still stuck.** Old process hung"));
    }

    #[test]
    fn last_working_message_tracks_previous() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "first output".to_string());
        store.update_message(1, "second output".to_string());

        let status = store.get(1).unwrap();
        // last_working_message is the previous message
        assert_eq!(status.last_working_message.as_deref(), Some("first output"));
    }
}
