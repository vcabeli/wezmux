use crate::pane::PaneId;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

const MAX_NOTIFICATIONS: usize = 1000;

#[derive(Debug, Clone)]
pub struct Notification {
    pub pane_id: PaneId,
    pub workspace: String,
    pub title: String,
    pub body: String,
    pub created: Instant,
    pub unread: bool,
}

#[derive(Debug, Default)]
pub struct NotificationStore {
    notifications: VecDeque<Notification>,
    unread_by_pane: HashMap<PaneId, usize>,
    unread_by_workspace: HashMap<String, usize>,
}

impl NotificationStore {
    pub fn add_notification(
        &mut self,
        pane_id: PaneId,
        workspace: String,
        title: String,
        body: String,
        unread: bool,
    ) {
        // Deduplicate: if the most recent notification for this pane has the
        // same title and body, skip adding a duplicate.
        if let Some(latest) = self.notifications.iter().find(|n| n.pane_id == pane_id) {
            if latest.title == title && latest.body == body {
                return;
            }
        }

        let notification = Notification {
            pane_id,
            workspace: workspace.clone(),
            title,
            body,
            created: Instant::now(),
            unread,
        };

        self.notifications.push_front(notification);
        if unread {
            *self.unread_by_pane.entry(pane_id).or_insert(0) += 1;
            *self.unread_by_workspace.entry(workspace).or_insert(0) += 1;
        }

        while self.notifications.len() > MAX_NOTIFICATIONS {
            if let Some(evicted) = self.notifications.pop_back() {
                if evicted.unread {
                    self.adjust_unread_counts(evicted.pane_id, &evicted.workspace, -1);
                }
            }
        }
    }

    pub fn mark_pane_read(&mut self, pane_id: PaneId) {
        let mut workspaces = vec![];

        for notification in self.notifications.iter_mut() {
            if notification.pane_id == pane_id && notification.unread {
                notification.unread = false;
                workspaces.push(notification.workspace.clone());
            }
        }

        for workspace in workspaces {
            self.adjust_unread_counts(pane_id, &workspace, -1);
        }
    }

    pub fn has_unread(&self, pane_id: PaneId) -> bool {
        self.unread_by_pane.get(&pane_id).copied().unwrap_or(0) > 0
    }

    pub fn most_recent_unread_pane(&self) -> Option<PaneId> {
        self.notifications
            .iter()
            .find(|notification| notification.unread)
            .map(|notification| notification.pane_id)
    }

    pub fn latest_for_workspace(&self, workspace: &str) -> Option<&Notification> {
        self.notifications
            .iter()
            .find(|notification| notification.workspace == workspace)
    }

    pub fn unread_count(&self, workspace: &str) -> u32 {
        self.unread_by_workspace
            .get(workspace)
            .copied()
            .unwrap_or(0) as u32
    }

    pub fn iter(&self) -> impl Iterator<Item = &Notification> {
        self.notifications.iter()
    }

    fn adjust_unread_counts(&mut self, pane_id: PaneId, workspace: &str, delta: isize) {
        adjust_count(&mut self.unread_by_pane, pane_id, delta);
        adjust_count(&mut self.unread_by_workspace, workspace.to_string(), delta);
    }
}

fn adjust_count<K>(counts: &mut HashMap<K, usize>, key: K, delta: isize)
where
    K: Eq + std::hash::Hash + Clone,
{
    let Some(count) = counts.get_mut(&key) else {
        return;
    };

    if delta.is_negative() {
        *count = count.saturating_sub(delta.unsigned_abs());
    } else {
        *count += delta as usize;
    }

    if *count == 0 {
        counts.remove(&key);
    }
}

#[cfg(test)]
mod test {
    use super::NotificationStore;

    #[test]
    fn unread_counts_and_mark_read_work() {
        let mut store = NotificationStore::default();
        store.add_notification(
            1,
            "alpha".to_string(),
            "done".to_string(),
            "body".to_string(),
            true,
        );
        store.add_notification(
            1,
            "alpha".to_string(),
            "done".to_string(),
            "body".to_string(),
            true,
        );
        store.add_notification(
            2,
            "beta".to_string(),
            "done".to_string(),
            "body".to_string(),
            true,
        );

        assert!(store.has_unread(1));
        assert_eq!(store.unread_count("alpha"), 2);
        assert_eq!(store.unread_count("beta"), 1);

        store.mark_pane_read(1);

        assert!(!store.has_unread(1));
        assert_eq!(store.unread_count("alpha"), 0);
        assert_eq!(store.unread_count("beta"), 1);
    }

    #[test]
    fn notification_store_is_capped() {
        let mut store = NotificationStore::default();
        for idx in 0..1100 {
            store.add_notification(
                idx,
                "alpha".to_string(),
                format!("title-{idx}"),
                "body".to_string(),
                true,
            );
        }

        assert_eq!(store.unread_count("alpha"), 1000);
        assert_eq!(store.most_recent_unread_pane(), Some(1099));
    }
}
