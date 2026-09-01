// Package feed is the session state feed that backs the dashboard. Claude Code
// hooks invoke `claude-status hook`, which writes one small JSON file per
// session here; the TUI reads them back. This mirrors how the wezmux wrapper
// drives its sidebar (Claude Code hooks → status), but writes to a file the
// out-of-process TUI can read instead of OSC sequences only the GUI sees.
//
// Hooks fire regardless of how `claude` was launched (alias, --resume,
// --continue, worktree, any terminal), which is why this replaces the old
// ps/argv discovery — that only worked when a session id appeared in argv.
package feed

import (
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"time"
)

// StaleAfter drops state files we haven't heard from in this long, so a session
// that crashed without firing SessionEnd eventually disappears instead of
// lingering forever. Generous, so a long idle/awaiting session isn't culled.
const StaleAfter = 12 * time.Hour

// Status values written by the hook.
const (
	StatusRunning  = "running"
	StatusAwaiting = "awaiting"
	StatusIdle     = "idle"
)

// State is one session's latest state, persisted as <session_id>.json.
type State struct {
	SessionID      string    `json:"session_id"`
	Cwd            string    `json:"cwd"`
	TranscriptPath string    `json:"transcript_path"`
	Status         string    `json:"status"`
	LastTool       string    `json:"last_tool"`
	UpdatedAt      time.Time `json:"updated_at"`
}

// Dir is where per-session state files live.
func Dir() string {
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".cache", "claude-status", "sessions")
}

func path(sessionID string) string { return filepath.Join(Dir(), sessionID+".json") }

// hookInput is the subset of the Claude Code hook stdin payload we consume.
type hookInput struct {
	SessionID      string `json:"session_id"`
	Cwd            string `json:"cwd"`
	TranscriptPath string `json:"transcript_path"`
	HookEventName  string `json:"hook_event_name"`
	ToolName       string `json:"tool_name"`
}

// statusFor maps a hook event to a status. The bool is false for events that
// shouldn't change status, so the previous value is preserved.
func statusFor(event, tool string) (string, bool) {
	switch event {
	case "UserPromptSubmit", "PostToolUse":
		return StatusRunning, true
	case "PreToolUse":
		if tool == "AskUserQuestion" {
			return StatusAwaiting, true
		}
		return StatusRunning, true
	case "Notification":
		// Claude Code fires this when it wants attention (permission, plan
		// approval, idle prompt) — treat as awaiting input.
		return StatusAwaiting, true
	case "Stop", "SessionStart":
		return StatusIdle, true
	default:
		return "", false
	}
}

// RunHook is the entry point for `claude-status hook`: it reads one Claude Code
// hook event from r and updates that session's state file. It never returns a
// hard error to the caller — a failing hook must not disrupt the Claude session.
func RunHook(r io.Reader) {
	raw, err := io.ReadAll(r)
	if err != nil {
		return
	}
	var in hookInput
	if err := json.Unmarshal(raw, &in); err != nil || in.SessionID == "" {
		return
	}

	if in.HookEventName == "SessionEnd" {
		_ = os.Remove(path(in.SessionID))
		return
	}

	st := State{SessionID: in.SessionID, UpdatedAt: time.Now()}
	if prev, ok := loadOne(in.SessionID); ok {
		st.Cwd, st.TranscriptPath, st.LastTool, st.Status =
			prev.Cwd, prev.TranscriptPath, prev.LastTool, prev.Status
	}
	if in.Cwd != "" {
		st.Cwd = in.Cwd
	}
	if in.TranscriptPath != "" {
		st.TranscriptPath = in.TranscriptPath
	}
	if in.ToolName != "" {
		st.LastTool = in.ToolName
	}
	if status, ok := statusFor(in.HookEventName, in.ToolName); ok {
		st.Status = status
	}
	if st.Status == "" {
		st.Status = StatusRunning
	}
	_ = writeOne(st)
}

func writeOne(st State) error {
	dir := Dir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	b, err := json.Marshal(st)
	if err != nil {
		return err
	}
	// Atomic: write to a temp file in the same dir, then rename over the target
	// so a concurrent reader never sees a half-written file.
	tmp, err := os.CreateTemp(dir, st.SessionID+".*.tmp")
	if err != nil {
		return err
	}
	tmpName := tmp.Name()
	if _, err := tmp.Write(b); err != nil {
		tmp.Close()
		os.Remove(tmpName)
		return err
	}
	if err := tmp.Close(); err != nil {
		os.Remove(tmpName)
		return err
	}
	return os.Rename(tmpName, path(st.SessionID))
}

func loadOne(sessionID string) (State, bool) {
	b, err := os.ReadFile(path(sessionID))
	if err != nil {
		return State{}, false
	}
	var st State
	if err := json.Unmarshal(b, &st); err != nil {
		return State{}, false
	}
	return st, true
}

// Load returns every non-stale session state on disk.
func Load() []State {
	entries, err := os.ReadDir(Dir())
	if err != nil {
		return nil
	}
	out := make([]State, 0, len(entries))
	for _, e := range entries {
		if e.IsDir() || filepath.Ext(e.Name()) != ".json" {
			continue
		}
		var st State
		b, err := os.ReadFile(filepath.Join(Dir(), e.Name()))
		if err != nil {
			continue
		}
		if json.Unmarshal(b, &st) != nil || st.SessionID == "" {
			continue
		}
		if time.Since(st.UpdatedAt) > StaleAfter {
			continue
		}
		out = append(out, st)
	}
	return out
}
