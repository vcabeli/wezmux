package registry_test

import (
	"testing"
	"time"

	"claude-status/internal/feed"
	"claude-status/internal/registry"
)

func st(sid, cwd, tool, status string, updated time.Time) feed.State {
	return feed.State{
		SessionID:      sid,
		Cwd:            cwd,
		TranscriptPath: "/t/" + sid + ".jsonl",
		LastTool:       tool,
		Status:         status,
		UpdatedAt:      updated,
	}
}

func TestReconcilePopulatesFields(t *testing.T) {
	r := registry.New()
	r.Reconcile([]feed.State{st("sid", "/Users/me/work/my-app", "Bash", feed.StatusRunning, time.Now())})

	snap := r.Snapshot()
	if len(snap) != 1 {
		t.Fatalf("snapshot len = %d, want 1", len(snap))
	}
	s := snap[0]
	if s.ID != "sid" || s.Cwd != "/Users/me/work/my-app" {
		t.Errorf("id/cwd = %q/%q", s.ID, s.Cwd)
	}
	if s.ProjectDir != "my-app" {
		t.Errorf("ProjectDir = %q, want base of cwd", s.ProjectDir)
	}
	if s.LastTool != "Bash" {
		t.Errorf("LastTool = %q", s.LastTool)
	}
	if s.TranscriptPath != "/t/sid.jsonl" {
		t.Errorf("TranscriptPath = %q", s.TranscriptPath)
	}
}

func TestReconcileMapsStatusFromFeed(t *testing.T) {
	r := registry.New()
	now := time.Now()
	r.Reconcile([]feed.State{
		st("run", "/a", "", feed.StatusRunning, now),
		st("wait", "/b", "", feed.StatusAwaiting, now),
		st("idle", "/c", "", feed.StatusIdle, now),
	})

	byID := map[string]registry.Status{}
	for _, s := range r.Snapshot() {
		byID[s.ID] = s.Status
	}
	if byID["run"] != registry.StatusRunning {
		t.Errorf("run = %v, want running", byID["run"])
	}
	if byID["wait"] != registry.StatusAwaiting {
		t.Errorf("wait = %v, want awaiting", byID["wait"])
	}
	if byID["idle"] != registry.StatusIdle {
		t.Errorf("idle = %v, want idle", byID["idle"])
	}
}

func TestReconcileEndsVanishedSessions(t *testing.T) {
	r := registry.New()
	now := time.Now()
	r.Reconcile([]feed.State{st("a", "/a", "", feed.StatusRunning, now), st("b", "/b", "", feed.StatusRunning, now)})

	// Next read only sees "b" — "a"'s state file was removed (SessionEnd) or aged out.
	r.Reconcile([]feed.State{st("b", "/b", "", feed.StatusRunning, now)})

	snap := r.Snapshot()
	if len(snap) != 1 || snap[0].ID != "b" {
		t.Fatalf("snapshot = %+v, want only b active (a vanished -> ended)", snap)
	}
}

func TestReconcileIgnoresEmptySessionID(t *testing.T) {
	r := registry.New()
	r.Reconcile([]feed.State{st("", "/a", "", feed.StatusRunning, time.Now())})
	if len(r.Snapshot()) != 0 {
		t.Error("empty session id created a session")
	}
}

func TestReconcileReturnsChangedOnFirstSightAndAdvance(t *testing.T) {
	r := registry.New()
	t0 := time.Now().Add(-time.Minute)

	// First sighting -> changed (needs a summary).
	changed := r.Reconcile([]feed.State{st("sid", "/a", "", feed.StatusRunning, t0)})
	if len(changed) != 1 {
		t.Fatalf("first sight changed = %d, want 1", len(changed))
	}
	r.SetSummary("sid", "doing a thing")

	// Same UpdatedAt, already summarised -> not changed.
	if c := r.Reconcile([]feed.State{st("sid", "/a", "", feed.StatusRunning, t0)}); len(c) != 0 {
		t.Errorf("unchanged state changed = %d, want 0", len(c))
	}

	// Advanced -> changed again.
	if c := r.Reconcile([]feed.State{st("sid", "/a", "", feed.StatusRunning, t0.Add(time.Second))}); len(c) != 1 {
		t.Errorf("advanced state changed = %d, want 1", len(c))
	}
}

func TestReconcileNoSummaryWithoutTranscriptPath(t *testing.T) {
	r := registry.New()
	s := feed.State{SessionID: "sid", Cwd: "/a", Status: feed.StatusRunning, UpdatedAt: time.Now()}
	if c := r.Reconcile([]feed.State{s}); len(c) != 0 {
		t.Errorf("changed = %d, want 0 when no transcript path to summarise from", len(c))
	}
}

func TestSetSummaryClearsDirty(t *testing.T) {
	r := registry.New()
	r.Reconcile([]feed.State{st("sid", "/a", "", feed.StatusRunning, time.Now())})
	r.SetSummary("sid", "fixing flaky test")

	snap := r.Snapshot()
	if len(snap) != 1 {
		t.Fatalf("snapshot len = %d, want 1", len(snap))
	}
	if snap[0].Summary != "fixing flaky test" {
		t.Errorf("Summary = %q", snap[0].Summary)
	}
	if snap[0].Dirty {
		t.Error("Dirty = true, want false after SetSummary")
	}
}

func TestSetSummaryUnknownSessionIsNoOp(t *testing.T) {
	r := registry.New()
	r.SetSummary("ghost", "x") // must not panic or create a session
	if len(r.Snapshot()) != 0 {
		t.Error("SetSummary on unknown id created a session")
	}
}

func TestSnapshotFiltersEndedAndSortsRecentFirst(t *testing.T) {
	r := registry.New()
	base := time.Now()
	r.Reconcile([]feed.State{
		st("old", "/old", "", feed.StatusIdle, base.Add(-200*time.Second)),
		st("new", "/new", "", feed.StatusRunning, base),
		st("gone", "/gone", "", feed.StatusIdle, base.Add(-100*time.Second)),
	})
	// "gone" disappears on the next read.
	r.Reconcile([]feed.State{
		st("old", "/old", "", feed.StatusIdle, base.Add(-200*time.Second)),
		st("new", "/new", "", feed.StatusRunning, base),
	})

	snap := r.Snapshot()
	if len(snap) != 2 {
		t.Fatalf("snapshot len = %d, want 2 (ended session filtered out)", len(snap))
	}
	if snap[0].ID != "new" || snap[1].ID != "old" {
		t.Errorf("order = [%s, %s], want most-recent-first [new, old]", snap[0].ID, snap[1].ID)
	}
}

func TestSnapshotReturnsCopies(t *testing.T) {
	r := registry.New()
	r.Reconcile([]feed.State{st("sid", "/a", "", feed.StatusRunning, time.Now())})
	snap := r.Snapshot()
	snap[0].Summary = "mutated copy"
	// Mutating the snapshot must not leak back into the registry.
	if again := r.Snapshot(); again[0].Summary != "" {
		t.Errorf("Snapshot returned a live pointer; registry Summary = %q", again[0].Summary)
	}
}
