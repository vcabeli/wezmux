use crate::pane::PaneId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const EXPIRY: Duration = Duration::from_secs(600); // 10 min

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
                updated: Instant::now(),
            });
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
        self.statuses.remove(&pane_id);
        self.generation += 1;
    }

    pub fn get(&self, pane_id: PaneId) -> Option<&AgentPaneStatus> {
        self.statuses
            .get(&pane_id)
            .filter(|s| s.updated.elapsed() < EXPIRY)
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
    fn clear_removes_entry() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.clear(1);
        assert!(store.get(1).is_none());
    }

    #[test]
    fn message_creates_entry_if_missing() {
        let mut store = AgentStatusStore::default();
        store.update_message(1, "orphan".to_string());
        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Working);
        assert_eq!(status.message.as_deref(), Some("orphan"));
    }
}
