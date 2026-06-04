package summarizer

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"claude-status/internal/creds"
	"claude-status/internal/transcript"
)

func TestParseMessagesResponse(t *testing.T) {
	got, err := parseMessagesResponse([]byte(`{"content":[{"type":"text","text":" hi there "}]}`))
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if got != "hi there" {
		t.Fatalf("got %q, want %q", got, "hi there")
	}

	if _, err := parseMessagesResponse([]byte(`{"content":[{"type":"tool_use"}]}`)); err == nil {
		t.Fatal("want error for response without text content")
	}
	if _, err := parseMessagesResponse([]byte(`not json`)); err == nil {
		t.Fatal("want error for invalid JSON")
	}
}

func TestFallback(t *testing.T) {
	if got := fallback(&transcript.Snapshot{Slug: "fix-the-bug"}); got != "fix the bug" {
		t.Fatalf("got %q, want %q", got, "fix the bug")
	}
	if got := fallback(&transcript.Snapshot{}); got != "[no summary]" {
		t.Fatalf("got %q, want %q", got, "[no summary]")
	}
}

// captureServer records the request the summarizer sends and replies with a
// valid messages response.
func captureServer(t *testing.T) (*httptest.Server, *http.Request, *[]byte) {
	var req http.Request
	var body []byte
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		req = *r
		body, _ = io.ReadAll(r.Body)
		w.Write([]byte(`{"content":[{"type":"text","text":"summary"}]}`))
	}))
	t.Cleanup(srv.Close)
	return srv, &req, &body
}

func TestCallMessagesOAuthHeaders(t *testing.T) {
	srv, req, body := captureServer(t)
	s := New()
	s.apiURL = srv.URL

	auth := &creds.Auth{
		Method: creds.MethodOAuth,
		OAuth:  &creds.Creds{AccessToken: "tok-123", ExpiresAt: time.Now().Add(time.Hour)},
	}
	got, err := s.callMessages(auth, &transcript.Snapshot{Slug: "test"})
	if err != nil {
		t.Fatalf("callMessages: %v", err)
	}
	if got != "summary" {
		t.Fatalf("got %q, want %q", got, "summary")
	}
	if h := req.Header.Get("Authorization"); h != "Bearer tok-123" {
		t.Fatalf("Authorization = %q, want Bearer tok-123", h)
	}
	if h := req.Header.Get("anthropic-beta"); h != anthropicBeta {
		t.Fatalf("anthropic-beta = %q, want %q", h, anthropicBeta)
	}
	if h := req.Header.Get("x-api-key"); h != "" {
		t.Fatalf("x-api-key = %q, want empty on OAuth path", h)
	}
	var payload struct {
		System string `json:"system"`
	}
	if err := json.Unmarshal(*body, &payload); err != nil {
		t.Fatalf("unmarshal request body: %v", err)
	}
	if !strings.HasPrefix(payload.System, claudeCodeIdent) {
		t.Fatalf("system prompt must start with the Claude Code identity, got %q", payload.System)
	}
}

func TestCallMessagesAPIKeyHeaders(t *testing.T) {
	srv, req, _ := captureServer(t)
	s := New()
	s.apiURL = srv.URL

	auth := &creds.Auth{Method: creds.MethodAPIKey, APIKey: "sk-test"}
	if _, err := s.callMessages(auth, &transcript.Snapshot{Slug: "test"}); err != nil {
		t.Fatalf("callMessages: %v", err)
	}
	if h := req.Header.Get("x-api-key"); h != "sk-test" {
		t.Fatalf("x-api-key = %q, want sk-test", h)
	}
	if h := req.Header.Get("Authorization"); h != "" {
		t.Fatalf("Authorization = %q, want empty on API-key path", h)
	}
	if h := req.Header.Get("anthropic-beta"); h != "" {
		t.Fatalf("anthropic-beta = %q, want empty on API-key path", h)
	}
}
