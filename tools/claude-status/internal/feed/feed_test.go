package feed

import (
	"strings"
	"testing"
	"time"
)

func TestStatusFor(t *testing.T) {
	cases := []struct {
		event, tool string
		want        string
		ok          bool
	}{
		{"UserPromptSubmit", "", StatusRunning, true},
		{"PostToolUse", "Bash", StatusRunning, true},
		{"PreToolUse", "Edit", StatusRunning, true},
		{"PreToolUse", "AskUserQuestion", StatusAwaiting, true},
		{"Notification", "", StatusAwaiting, true},
		{"Stop", "", StatusIdle, true},
		{"SessionStart", "", StatusIdle, true},
		{"SubagentStop", "", "", false}, // must not change status
	}
	for _, c := range cases {
		got, ok := statusFor(c.event, c.tool)
		if got != c.want || ok != c.ok {
			t.Errorf("statusFor(%q,%q) = (%q,%v), want (%q,%v)", c.event, c.tool, got, ok, c.want, c.ok)
		}
	}
}

func TestRunHookWritesAndLoads(t *testing.T) {
	t.Setenv("HOME", t.TempDir())

	RunHook(strings.NewReader(`{
		"session_id":"sid-1","cwd":"/a/proj","transcript_path":"/t/sid-1.jsonl",
		"hook_event_name":"PreToolUse","tool_name":"Bash"
	}`))

	got := Load()
	if len(got) != 1 {
		t.Fatalf("Load len = %d, want 1", len(got))
	}
	s := got[0]
	if s.SessionID != "sid-1" || s.Cwd != "/a/proj" || s.TranscriptPath != "/t/sid-1.jsonl" {
		t.Errorf("fields = %+v", s)
	}
	if s.Status != StatusRunning || s.LastTool != "Bash" {
		t.Errorf("status/tool = %q/%q, want running/Bash", s.Status, s.LastTool)
	}
}

func TestRunHookPreservesFieldsAcrossEvents(t *testing.T) {
	t.Setenv("HOME", t.TempDir())

	// First a tool use sets cwd/transcript/tool.
	RunHook(strings.NewReader(`{"session_id":"s","cwd":"/p","transcript_path":"/t.jsonl","hook_event_name":"PreToolUse","tool_name":"Edit"}`))
	// Then a Notification with no cwd/tool must not wipe them, but flips status.
	RunHook(strings.NewReader(`{"session_id":"s","hook_event_name":"Notification"}`))

	got := Load()
	if len(got) != 1 {
		t.Fatalf("Load len = %d, want 1", len(got))
	}
	s := got[0]
	if s.Cwd != "/p" || s.TranscriptPath != "/t.jsonl" || s.LastTool != "Edit" {
		t.Errorf("event without cwd/tool clobbered state: %+v", s)
	}
	if s.Status != StatusAwaiting {
		t.Errorf("status = %q, want awaiting after Notification", s.Status)
	}
}

func TestRunHookSessionEndRemoves(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	RunHook(strings.NewReader(`{"session_id":"s","cwd":"/p","hook_event_name":"UserPromptSubmit"}`))
	if len(Load()) != 1 {
		t.Fatal("expected one session before SessionEnd")
	}
	RunHook(strings.NewReader(`{"session_id":"s","hook_event_name":"SessionEnd"}`))
	if len(Load()) != 0 {
		t.Error("SessionEnd should have removed the session state")
	}
}

func TestRunHookIgnoresGarbage(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	RunHook(strings.NewReader(`not json`))
	RunHook(strings.NewReader(`{"hook_event_name":"Stop"}`)) // no session_id
	if len(Load()) != 0 {
		t.Error("garbage / session-less events must not create state")
	}
}

func TestLoadSkipsStale(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	RunHook(strings.NewReader(`{"session_id":"old","cwd":"/p","hook_event_name":"Stop"}`))
	// Backdate the file past StaleAfter by rewriting it directly.
	st, ok := loadOne("old")
	if !ok {
		t.Fatal("expected to load the session we just wrote")
	}
	st.UpdatedAt = time.Now().Add(-StaleAfter - time.Hour)
	if err := writeOne(st); err != nil {
		t.Fatal(err)
	}
	if len(Load()) != 0 {
		t.Error("stale session should be filtered out of Load")
	}
}
