package transcript

import (
	"os"
	"path/filepath"
	"testing"
)

func TestEncodeCwd(t *testing.T) {
	cases := map[string]string{
		"/Users/me/work/proj": "-Users-me-work-proj",
		"relative/path":       "relative-path",
		"":                    "",
	}
	for in, want := range cases {
		if got := encodeCwd(in); got != want {
			t.Errorf("encodeCwd(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestExtractUserText(t *testing.T) {
	if got := extractUserText([]byte(`{"content":"hello world"}`)); got != "hello world" {
		t.Errorf("string content = %q, want %q", got, "hello world")
	}
	// Array-shaped content is not a plain string and yields "".
	if got := extractUserText([]byte(`{"content":[{"type":"text","text":"x"}]}`)); got != "" {
		t.Errorf("array content = %q, want empty", got)
	}
	if got := extractUserText([]byte(`not json`)); got != "" {
		t.Errorf("invalid json = %q, want empty", got)
	}
}

func TestExtractAssistant(t *testing.T) {
	raw := []byte(`{"content":[
		{"type":"text","text":"working on it"},
		{"type":"tool_use","name":"Bash"}
	]}`)
	text, tool := extractAssistant(raw)
	if text != "working on it" {
		t.Errorf("text = %q", text)
	}
	if tool != "Bash" {
		t.Errorf("tool = %q", tool)
	}

	// Last non-empty text and tool_use win.
	multi := []byte(`{"content":[
		{"type":"text","text":"first"},
		{"type":"tool_use","name":"Read"},
		{"type":"text","text":"second"},
		{"type":"tool_use","name":"Edit"}
	]}`)
	text, tool = extractAssistant(multi)
	if text != "second" || tool != "Edit" {
		t.Errorf("multi-block: text=%q tool=%q, want second/Edit", text, tool)
	}
}

// writeTranscript lays out a transcript file at the path Tail expects, using a
// temporary HOME so the test never touches the real ~/.claude tree.
func writeTranscript(t *testing.T, cwd, sid, body string) {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	dir := filepath.Join(home, ".claude", "projects", encodeCwd(cwd))
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, sid+".jsonl"), []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestTailExtractsPreviewFields(t *testing.T) {
	cwd, sid := "/Users/me/proj", "sess-1"
	body := `{"type":"summary","message":{}}
{"type":"user","slug":"my-cool-slug","message":{"content":"first question"}}
{"type":"assistant","message":{"content":[{"type":"text","text":"an answer"},{"type":"tool_use","name":"Grep"}]}}
{"type":"user","message":{"content":"second question"}}
`
	writeTranscript(t, cwd, sid, body)

	snap, err := Tail(cwd, sid, 10)
	if err != nil {
		t.Fatal(err)
	}
	if snap.Slug != "my-cool-slug" {
		t.Errorf("Slug = %q", snap.Slug)
	}
	if snap.LastUserText != "second question" {
		t.Errorf("LastUserText = %q, want last user line", snap.LastUserText)
	}
	if snap.LastAssistant != "an answer" {
		t.Errorf("LastAssistant = %q", snap.LastAssistant)
	}
	if snap.LastToolUse != "Grep" {
		t.Errorf("LastToolUse = %q", snap.LastToolUse)
	}
}

func TestTailHonorsRingLimit(t *testing.T) {
	cwd, sid := "/x/proj", "sess-2"
	body := `{"type":"user","message":{"content":"a"}}
{"type":"user","message":{"content":"b"}}
{"type":"user","message":{"content":"c"}}
{"type":"user","message":{"content":"d"}}
{"type":"user","message":{"content":"e"}}
`
	writeTranscript(t, cwd, sid, body)

	snap, err := Tail(cwd, sid, 2)
	if err != nil {
		t.Fatal(err)
	}
	if len(snap.Entries) != 2 {
		t.Fatalf("Entries len = %d, want 2 (ring should keep only the last n)", len(snap.Entries))
	}
	// The ring keeps the most recent entries, so the last user text is "e".
	if snap.LastUserText != "e" {
		t.Errorf("LastUserText = %q, want %q", snap.LastUserText, "e")
	}
}

func TestTailMissingFileErrors(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	if _, err := Tail("/no/such", "nope", 5); err == nil {
		t.Error("Tail on missing transcript returned nil error, want an error")
	}
}
