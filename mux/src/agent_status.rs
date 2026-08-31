use crate::pane::PaneId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentStatus {
    Working,
    Idle,
    NeedsInput,
}

#[derive(Debug, Clone)]
pub struct AgentPaneStatus {
    pub status: AgentStatus,
    /// Agent-owned conversation title, when the integration can provide one.
    /// This is deliberately separate from the terminal tab title.
    pub conversation_title: Option<String>,
    pub message: Option<String>,
    pub tool: Option<String>,
    /// The last message received while the agent was in Working state.
    /// Preserved across status transitions so the sidebar keeps showing
    /// useful output even after the agent goes idle.
    pub last_working_message: Option<String>,
    /// Number of subagents currently running within this agent session
    /// (e.g. Claude Code's parallel Agent tool invocations).
    pub subagent_count: u32,
    /// Number of background tasks (Bash run_in_background, Monitor) the
    /// session has spawned.  Sourced from the `background_tasks` field on
    /// Claude Code's Stop / SubagentStop hook input (v2.1.145+).
    pub background_tasks_count: u32,
    /// Number of scheduled tasks (`/loop`, CronCreate) the session holds.
    /// Sourced from the `session_crons` field on Claude Code's Stop /
    /// SubagentStop hook input (v2.1.145+).
    pub session_crons_count: u32,
    pub updated: Instant,
}

#[derive(Debug, Default)]
pub struct AgentStatusStore {
    statuses: HashMap<PaneId, AgentPaneStatus>,
    generation: u64,
}

impl AgentStatusStore {
    /// Fetch the entry for `pane_id`, creating it with `default_status` if
    /// absent.  The returned flag is true when the entry was just created,
    /// which is itself a sidebar-visible change even if every field the
    /// caller goes on to write happens to match the default.
    fn entry_mut(
        &mut self,
        pane_id: PaneId,
        default_status: AgentStatus,
    ) -> (&mut AgentPaneStatus, bool) {
        let is_new = !self.statuses.contains_key(&pane_id);
        let entry = self
            .statuses
            .entry(pane_id)
            .or_insert_with(|| AgentPaneStatus {
                status: default_status,
                conversation_title: None,
                message: None,
                tool: None,
                last_working_message: None,
                subagent_count: 0,
                background_tasks_count: 0,
                session_crons_count: 0,
                updated: Instant::now(),
            });
        (entry, is_new)
    }

    // Every mutator below follows the same contract:
    //
    // * `updated` is refreshed on every call, because the NeedsInput sticky
    //   guard in `update_status` reads it as "when did we last hear from this
    //   agent", and that must not depend on whether the value changed.
    // * `generation` moves *only* when a sidebar-visible field actually
    //   changed.  The sidebar reads it as "your cached cards are stale" and
    //   a rebuild is expensive, so writing the same value twice must be
    //   invisible.  Agents re-send unchanged values constantly — a Stop hook
    //   reports `session_crons` and `background_tasks` every time, and
    //   consecutive tool calls report the same tool name — so treating a
    //   no-op write as a change means rebuilding for nothing thousands of
    //   times per session.

    pub fn update_status(&mut self, pane_id: PaneId, status: AgentStatus) {
        let (entry, is_new) = self.entry_mut(pane_id, status.clone());

        // Guard against the Stop/Notification hook race: when Claude stops
        // for plan approval, both hooks fire concurrently.  If Notification
        // sets NeedsInput first and Stop's Idle arrives within a short
        // window, drop the Idle so the sidebar keeps showing NeedsInput.
        if matches!(status, AgentStatus::Idle)
            && matches!(entry.status, AgentStatus::NeedsInput)
            && entry.updated.elapsed() < Duration::from_secs(3)
        {
            return;
        }

        let mut changed = is_new;

        // When leaving Working, snapshot the current message as the
        // last working output so the sidebar shows it while idle.
        if matches!(entry.status, AgentStatus::Working) && !matches!(status, AgentStatus::Working) {
            if let Some(msg) = entry.message.clone() {
                if entry.last_working_message.as_ref() != Some(&msg) {
                    entry.last_working_message = Some(msg);
                    changed = true;
                }
            }
        }
        // When entering Working, clear the current message so stale
        // status labels (e.g. "Claude is waiting") don't persist.
        // Keep last_working_message as a fallback preview until new
        // output arrives — clearing it causes blank sidebar cards.
        if matches!(status, AgentStatus::Working) && !matches!(entry.status, AgentStatus::Working) {
            if entry.message.is_some() {
                entry.message = None;
                changed = true;
            }
        }
        if entry.status != status {
            entry.status = status;
            changed = true;
        }
        entry.updated = Instant::now();

        if changed {
            self.generation += 1;
        }
    }

    pub fn update_message(&mut self, pane_id: PaneId, message: String) {
        let (entry, is_new) = self.entry_mut(pane_id, AgentStatus::Working);
        let changed = is_new || entry.message.as_deref() != Some(message.as_str());
        entry.message = Some(message);
        entry.updated = Instant::now();
        if changed {
            self.generation += 1;
        }
    }

    pub fn update_conversation_title(&mut self, pane_id: PaneId, title: String) {
        let (entry, is_new) = self.entry_mut(pane_id, AgentStatus::Working);
        let changed = is_new || entry.conversation_title.as_deref() != Some(title.as_str());
        entry.conversation_title = Some(title);
        entry.updated = Instant::now();
        if changed {
            self.generation += 1;
        }
    }

    pub fn update_tool(&mut self, pane_id: PaneId, tool: String) {
        if let Some(entry) = self.statuses.get_mut(&pane_id) {
            let changed = entry.tool.as_deref() != Some(tool.as_str());
            entry.tool = Some(tool);
            entry.updated = Instant::now();
            if changed {
                self.generation += 1;
            }
        }
    }

    pub fn update_subagent_count(&mut self, pane_id: PaneId, count: u32) {
        let (entry, is_new) = self.entry_mut(pane_id, AgentStatus::Working);
        let changed = is_new || entry.subagent_count != count;
        entry.subagent_count = count;
        entry.updated = Instant::now();
        if changed {
            self.generation += 1;
        }
    }

    pub fn update_background_tasks_count(&mut self, pane_id: PaneId, count: u32) {
        let (entry, is_new) = self.entry_mut(pane_id, AgentStatus::Idle);
        let changed = is_new || entry.background_tasks_count != count;
        entry.background_tasks_count = count;
        entry.updated = Instant::now();
        if changed {
            self.generation += 1;
        }
    }

    pub fn update_session_crons_count(&mut self, pane_id: PaneId, count: u32) {
        let (entry, is_new) = self.entry_mut(pane_id, AgentStatus::Idle);
        let changed = is_new || entry.session_crons_count != count;
        entry.session_crons_count = count;
        entry.updated = Instant::now();
        if changed {
            self.generation += 1;
        }
    }

    pub fn clear(&mut self, pane_id: PaneId) {
        // Don't remove — preserve last known message/status so the sidebar
        // keeps showing agent info as long as the process is alive.
        // Only reset status to Idle; keep message and tool for context.
        //
        // Same contract as the mutators above: always refresh `updated`,
        // but only move the generation when the status actually changed.
        if let Some(entry) = self.statuses.get_mut(&pane_id) {
            let changed = !matches!(entry.status, AgentStatus::Idle);
            entry.status = AgentStatus::Idle;
            entry.updated = Instant::now();
            if changed {
                self.generation += 1;
            }
        }
    }

    pub fn remove(&mut self, pane_id: PaneId) {
        // Likewise: only signal a change if there was an entry to remove.
        // `sidebar_entries` calls this for every non-agent pane while it
        // rebuilds, so bumping unconditionally makes the rebuild invalidate
        // its own cache and repaint forever.
        if self.statuses.remove(&pane_id).is_some() {
            self.generation += 1;
        }
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
        store.update_conversation_title(1, "Fix sidebar titles".to_string());
        store.update_message(1, "Refactoring auth".to_string());
        store.update_tool(1, "Edit".to_string());

        let status = store.get(1).unwrap();
        assert_eq!(
            status.conversation_title.as_deref(),
            Some("Fix sidebar titles")
        );
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

    // The sidebar rebuild treats a generation bump as "your cached cards are
    // stale", and it calls remove/clear for panes it decides aren't agents
    // *while* it rebuilds.  If a no-op call bumps the generation, the rebuild
    // invalidates its own cache and the sidebar repaints forever, walking the
    // system process table for every pane on every frame.

    #[test]
    fn remove_of_absent_pane_does_not_bump_generation() {
        let mut store = AgentStatusStore::default();
        let before = store.generation();
        store.remove(42);
        assert_eq!(store.generation(), before);
    }

    #[test]
    fn clear_of_absent_pane_does_not_bump_generation() {
        let mut store = AgentStatusStore::default();
        let before = store.generation();
        store.clear(42);
        assert_eq!(store.generation(), before);
    }

    #[test]
    fn clear_of_already_idle_pane_does_not_bump_generation() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Idle);
        let before = store.generation();
        store.clear(1);
        assert_eq!(store.generation(), before);
    }

    #[test]
    fn repeated_identical_writes_do_not_bump_generation() {
        let mut store = AgentStatusStore::default();

        // Prime every field, then re-send exactly the same values.  Agents do
        // this constantly: a Stop hook reports session_crons/background_tasks
        // every time, and consecutive Bash calls report the same tool.
        store.update_status(1, AgentStatus::Working);
        store.update_conversation_title(1, "same title".to_string());
        store.update_message(1, "same".to_string());
        store.update_tool(1, "Bash".to_string());
        store.update_subagent_count(1, 2);
        store.update_background_tasks_count(1, 4);
        store.update_session_crons_count(1, 0);

        let settled = store.generation();

        for _ in 0..5 {
            store.update_status(1, AgentStatus::Working);
            store.update_conversation_title(1, "same title".to_string());
            store.update_message(1, "same".to_string());
            store.update_tool(1, "Bash".to_string());
            store.update_subagent_count(1, 2);
            store.update_background_tasks_count(1, 4);
            store.update_session_crons_count(1, 0);
        }

        assert_eq!(
            store.generation(),
            settled,
            "re-sending identical values must not invalidate the sidebar cache"
        );

        // ...but the values are still there.
        let status = store.get(1).unwrap();
        assert_eq!(status.message.as_deref(), Some("same"));
        assert_eq!(status.conversation_title.as_deref(), Some("same title"));
        assert_eq!(status.tool.as_deref(), Some("Bash"));
        assert_eq!(status.subagent_count, 2);
        assert_eq!(status.background_tasks_count, 4);
        assert_eq!(status.session_crons_count, 0);
    }

    #[test]
    fn each_field_change_bumps_generation() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_conversation_title(1, "a title".to_string());
        store.update_message(1, "a".to_string());
        store.update_tool(1, "Bash".to_string());
        store.update_subagent_count(1, 1);
        store.update_background_tasks_count(1, 1);
        store.update_session_crons_count(1, 1);

        // Each of these is a real change and must be visible to the sidebar.
        let mut prev = store.generation();
        for (label, apply) in [
            (
                "conversation title",
                Box::new(|s: &mut AgentStatusStore| {
                    s.update_conversation_title(1, "another title".to_string())
                }) as Box<dyn Fn(&mut AgentStatusStore)>,
            ),
            (
                "message",
                Box::new(|s: &mut AgentStatusStore| s.update_message(1, "b".to_string()))
                    as Box<dyn Fn(&mut AgentStatusStore)>,
            ),
            (
                "tool",
                Box::new(|s: &mut AgentStatusStore| s.update_tool(1, "Read".to_string())),
            ),
            (
                "subagents",
                Box::new(|s: &mut AgentStatusStore| s.update_subagent_count(1, 3)),
            ),
            (
                "background_tasks",
                Box::new(|s: &mut AgentStatusStore| s.update_background_tasks_count(1, 7)),
            ),
            (
                "session_crons",
                Box::new(|s: &mut AgentStatusStore| s.update_session_crons_count(1, 9)),
            ),
            (
                "status",
                Box::new(|s: &mut AgentStatusStore| s.update_status(1, AgentStatus::NeedsInput)),
            ),
        ] {
            apply(&mut store);
            assert!(
                store.generation() > prev,
                "changing {} should bump the generation",
                label
            );
            prev = store.generation();
        }
    }

    #[test]
    fn creating_an_entry_bumps_even_when_values_match_defaults() {
        // A brand new entry is a new sidebar card, so it counts as a change
        // even though `0` matches the default the entry is created with.
        let mut store = AgentStatusStore::default();
        let before = store.generation();
        store.update_session_crons_count(7, 0);
        assert!(store.generation() > before);
        assert!(store.get(7).is_some());
    }

    #[test]
    fn no_op_write_still_refreshes_updated() {
        // `updated` feeds the 3s NeedsInput sticky guard, so it must keep
        // advancing even when the generation doesn't.
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::NeedsInput);
        let first = store.get(1).unwrap().updated;
        let gen = store.generation();

        std::thread::sleep(std::time::Duration::from_millis(5));
        store.update_status(1, AgentStatus::NeedsInput);

        assert!(
            store.get(1).unwrap().updated > first,
            "a repeated status must still refresh `updated`"
        );
        assert_eq!(
            store.generation(),
            gen,
            "...without invalidating the sidebar cache"
        );
    }

    #[test]
    fn real_changes_still_bump_generation() {
        let mut store = AgentStatusStore::default();

        store.update_status(1, AgentStatus::Working);
        let after_status = store.generation();

        store.clear(1);
        let after_clear = store.generation();
        assert!(
            after_clear > after_status,
            "clearing a Working pane is a real change"
        );

        store.remove(1);
        assert!(
            store.generation() > after_clear,
            "removing a present pane is a real change"
        );
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
    fn last_working_message_snapshots_on_idle_transition() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "Refactoring auth module".to_string());
        store.update_message(1, "Done refactoring".to_string());

        // Transition to idle snapshots the current message
        store.update_status(1, AgentStatus::Idle);

        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Idle);
        // last_working_message is the message at the moment of transition
        assert_eq!(
            status.last_working_message.as_deref(),
            Some("Done refactoring")
        );
        // message is also still set
        assert_eq!(status.message.as_deref(), Some("Done refactoring"));
    }

    #[test]
    fn post_idle_message_does_not_overwrite_last_working_message() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "Useful output".to_string());
        store.update_status(1, AgentStatus::Idle);

        // Generic "waiting" message arrives after idle
        store.update_message(1, "Claude is waiting for your input".to_string());

        let status = store.get(1).unwrap();
        // Current message is the generic one
        assert_eq!(
            status.message.as_deref(),
            Some("Claude is waiting for your input")
        );
        // last_working_message preserves the useful output from Working
        assert_eq!(
            status.last_working_message.as_deref(),
            Some("Useful output")
        );
    }

    #[test]
    fn working_transition_clears_message_keeps_lwm() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "old output".to_string());
        store.update_status(1, AgentStatus::Idle);

        // Start a new working session
        store.update_status(1, AgentStatus::Working);

        let status = store.get(1).unwrap();
        // message is cleared (stale status label like "Claude finished")
        assert!(status.message.is_none());
        // last_working_message is preserved as fallback preview
        assert_eq!(status.last_working_message.as_deref(), Some("old output"));
    }

    #[test]
    fn messages_during_working_not_saved_to_lwm() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "first output".to_string());
        store.update_message(1, "second output".to_string());

        let status = store.get(1).unwrap();
        // During working, update_message only sets message, not lwm
        assert_eq!(status.message.as_deref(), Some("second output"));
        assert!(status.last_working_message.is_none());
    }

    #[test]
    fn message_during_needs_input_preserves_status() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::NeedsInput);

        // A message arrives while NeedsInput — status stays NeedsInput
        // (the message might be a label like "needs your approval",
        // not evidence that the agent resumed working)
        store.update_message(1, "needs your approval".to_string());

        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::NeedsInput);
        assert_eq!(status.message.as_deref(), Some("needs your approval"));
    }

    #[test]
    fn tool_during_needs_input_preserves_status() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_status(1, AgentStatus::NeedsInput);

        // Tool update during NeedsInput doesn't auto-transition
        store.update_tool(1, "Bash".to_string());

        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::NeedsInput);
    }

    #[test]
    fn idle_does_not_overwrite_recent_needs_input() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::NeedsInput);

        // Idle arrives immediately after (racing Stop hook)
        store.update_status(1, AgentStatus::Idle);

        let status = store.get(1).unwrap();
        // NeedsInput is preserved — the Idle was a race
        assert_eq!(status.status, AgentStatus::NeedsInput);
    }

    #[test]
    fn working_can_overwrite_needs_input() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::NeedsInput);

        // Working should always override NeedsInput (user answered)
        store.update_status(1, AgentStatus::Working);

        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Working);
    }

    #[test]
    fn preview_survives_idle_working_cycle() {
        let mut store = AgentStatusStore::default();

        // Working session produces output
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "Refactoring auth module".to_string());

        // Stop hook: idle + preview message
        store.update_status(1, AgentStatus::Idle);
        store.update_message(1, "Claude finished".to_string());

        // User submits new prompt → working
        store.update_status(1, AgentStatus::Working);

        let status = store.get(1).unwrap();
        // message is cleared (stale "Claude finished" label)
        assert!(status.message.is_none());
        // last_working_message survives as fallback preview
        assert_eq!(
            status.last_working_message.as_deref(),
            Some("Refactoring auth module")
        );
    }

    #[test]
    fn new_message_replaces_stale_lwm() {
        let mut store = AgentStatusStore::default();

        // First working session
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "old output".to_string());
        store.update_status(1, AgentStatus::Idle);

        // New working session — lwm preserved as fallback
        store.update_status(1, AgentStatus::Working);
        assert_eq!(
            store.get(1).unwrap().last_working_message.as_deref(),
            Some("old output")
        );

        // New output arrives — lwm should NOT leak into next idle
        store.update_message(1, "new output".to_string());
        store.update_status(1, AgentStatus::Idle);

        let status = store.get(1).unwrap();
        // lwm is now "new output", not "old output"
        assert_eq!(status.last_working_message.as_deref(), Some("new output"));
    }

    #[test]
    fn background_tasks_count_updates() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_background_tasks_count(1, 3);
        assert_eq!(store.get(1).unwrap().background_tasks_count, 3);
        store.update_background_tasks_count(1, 0);
        assert_eq!(store.get(1).unwrap().background_tasks_count, 0);
    }

    #[test]
    fn background_tasks_count_preserves_status() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_background_tasks_count(1, 2);
        // Updating the count must not flip the status back to Idle / Working
        assert_eq!(store.get(1).unwrap().status, AgentStatus::Working);
    }

    #[test]
    fn background_tasks_count_creates_entry_if_missing() {
        let mut store = AgentStatusStore::default();
        store.update_background_tasks_count(1, 4);
        let status = store.get(1).unwrap();
        // No prior status — create as Idle (these counts arrive on Stop hook,
        // i.e. after the agent has finished).
        assert_eq!(status.status, AgentStatus::Idle);
        assert_eq!(status.background_tasks_count, 4);
    }

    #[test]
    fn session_crons_count_updates() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_session_crons_count(1, 2);
        assert_eq!(store.get(1).unwrap().session_crons_count, 2);
    }

    #[test]
    fn counts_are_independent() {
        let mut store = AgentStatusStore::default();
        store.update_subagent_count(1, 1);
        store.update_background_tasks_count(1, 2);
        store.update_session_crons_count(1, 3);
        let status = store.get(1).unwrap();
        assert_eq!(status.subagent_count, 1);
        assert_eq!(status.background_tasks_count, 2);
        assert_eq!(status.session_crons_count, 3);
    }

    #[test]
    fn message_during_idle_does_not_transition() {
        let mut store = AgentStatusStore::default();
        store.update_status(1, AgentStatus::Working);
        store.update_message(1, "output".to_string());
        store.update_status(1, AgentStatus::Idle);

        // Post-idle message should NOT auto-transition to Working
        store.update_message(1, "generic status".to_string());

        let status = store.get(1).unwrap();
        assert_eq!(status.status, AgentStatus::Idle);
    }
}
