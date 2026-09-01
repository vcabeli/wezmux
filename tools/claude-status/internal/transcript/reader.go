package transcript

import (
	"bufio"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

type Entry struct {
	Type    string          `json:"type"`
	Slug    string          `json:"slug"`
	Message json.RawMessage `json:"message"`
}

type Snapshot struct {
	Slug          string
	LastUserText  string
	LastAssistant string
	LastToolUse   string
	Entries       []Entry
}

func encodeCwd(cwd string) string {
	return strings.ReplaceAll(cwd, "/", "-")
}

func Path(cwd, sessionID string) string {
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".claude", "projects", encodeCwd(cwd), sessionID+".jsonl")
}

// Tail reads the transcript at the canonical path for cwd+sessionID. Prefer
// TailPath with the hook-reported transcript_path — the cwd→path encoding is
// lossy for worktrees and paths containing dots.
func Tail(cwd, sessionID string, n int) (*Snapshot, error) {
	return TailPath(Path(cwd, sessionID), n)
}

// TailPath reads the last `n` user/assistant/last-prompt/ai-title entries from
// the transcript at `path` and extracts preview fields.
func TailPath(path string, n int) (*Snapshot, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()

	scanner := bufio.NewScanner(f)
	scanner.Buffer(make([]byte, 0, 64*1024), 8*1024*1024)

	ring := make([]Entry, 0, n)
	var slug string

	for scanner.Scan() {
		var e Entry
		if err := json.Unmarshal(scanner.Bytes(), &e); err != nil {
			continue
		}
		if e.Slug != "" {
			slug = e.Slug
		}
		switch e.Type {
		case "user", "assistant", "last-prompt", "ai-title":
			if len(ring) >= n {
				ring = ring[1:]
			}
			ring = append(ring, e)
		}
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}

	snap := &Snapshot{Slug: slug, Entries: ring}
	for _, e := range ring {
		switch e.Type {
		case "user":
			if t := extractUserText(e.Message); t != "" {
				snap.LastUserText = t
			}
		case "assistant":
			text, tool := extractAssistant(e.Message)
			if text != "" {
				snap.LastAssistant = text
			}
			if tool != "" {
				snap.LastToolUse = tool
			}
		}
	}
	return snap, nil
}

func extractUserText(raw json.RawMessage) string {
	var m struct {
		Content json.RawMessage `json:"content"`
	}
	if err := json.Unmarshal(raw, &m); err != nil {
		return ""
	}
	var s string
	if err := json.Unmarshal(m.Content, &s); err == nil {
		return s
	}
	return ""
}

func extractAssistant(raw json.RawMessage) (text, tool string) {
	var m struct {
		Content []struct {
			Type string `json:"type"`
			Text string `json:"text"`
			Name string `json:"name"`
		} `json:"content"`
	}
	if err := json.Unmarshal(raw, &m); err != nil {
		return "", ""
	}
	for _, c := range m.Content {
		if c.Type == "text" && c.Text != "" {
			text = c.Text
		}
		if c.Type == "tool_use" && c.Name != "" {
			tool = c.Name
		}
	}
	return
}
