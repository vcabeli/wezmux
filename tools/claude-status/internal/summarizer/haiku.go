package summarizer

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/aws/aws-sdk-go-v2/service/bedrockruntime"

	"claude-status/internal/creds"
	"claude-status/internal/transcript"
)

const (
	defaultAPIURL   = "https://api.anthropic.com/v1/messages"
	model           = "claude-haiku-4-5-20251001"
	debounce        = 5 * time.Second
	anthropicBeta   = "oauth-2025-04-20"
	claudeCodeIdent = "You are Claude Code, Anthropic's official CLI for Claude."
)

type Result struct {
	SessionID string
	Summary   string
	Err       error
}

type Summarizer struct {
	client *http.Client
	apiURL string

	mu      sync.Mutex
	pending map[string]*time.Timer

	// Bedrock client, built lazily on first use so AWS config loading never
	// runs for OAuth/API-key users. A load error is cached for the process
	// lifetime — summaries then degrade to the slug fallback.
	brOnce   sync.Once
	brClient *bedrockruntime.Client
	brErr    error
}

func New() *Summarizer {
	return &Summarizer{
		client:  &http.Client{Timeout: 20 * time.Second},
		apiURL:  defaultAPIURL,
		pending: map[string]*time.Timer{},
	}
}

// Request schedules a debounced summary for the session. onResult is called
// from a separate goroutine when the LLM responds (or on fallback / error).
func (s *Summarizer) Request(sessionID, transcriptPath string, onResult func(Result)) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if t, ok := s.pending[sessionID]; ok {
		t.Stop()
	}
	s.pending[sessionID] = time.AfterFunc(debounce, func() {
		s.mu.Lock()
		delete(s.pending, sessionID)
		s.mu.Unlock()

		summary, err := s.summarize(transcriptPath)
		onResult(Result{SessionID: sessionID, Summary: summary, Err: err})
	})
}

// summarize resolves the auth method and calls the model. Resolve-stage
// failures (missing keychain, expired token, AWS config load error) degrade
// to the slug fallback; only call-stage failures (HTTP / InvokeModel errors)
// propagate as errors.
func (s *Summarizer) summarize(transcriptPath string) (string, error) {
	snap, err := transcript.TailPath(transcriptPath, 20)
	if err != nil {
		return "", fmt.Errorf("tail transcript: %w", err)
	}
	auth, err := creds.Resolve()
	if err != nil {
		return fallback(snap), nil
	}
	switch auth.Method {
	case creds.MethodBedrock:
		client, err := s.bedrock(context.Background())
		if err != nil {
			return fallback(snap), nil
		}
		return s.callBedrock(context.Background(), client, auth.ModelID, snap)
	case creds.MethodOAuth:
		if auth.OAuth.Expired() {
			return fallback(snap), nil
		}
		return s.callMessages(auth, snap)
	default: // creds.MethodAPIKey
		return s.callMessages(auth, snap)
	}
}

// fallback returns a humanized slug so the UI keeps working without auth.
func fallback(snap *transcript.Snapshot) string {
	if snap.Slug != "" {
		return strings.ReplaceAll(snap.Slug, "-", " ")
	}
	return "[no summary]"
}

// callMessages POSTs to the Anthropic Messages API, authenticating with the
// OAuth Bearer token (plus the oauth beta header) or a plain x-api-key.
func (s *Summarizer) callMessages(auth *creds.Auth, snap *transcript.Snapshot) (string, error) {
	body, _ := json.Marshal(map[string]any{
		"model":      model,
		"max_tokens": 60,
		"system":     claudeCodeIdent,
		"messages": []map[string]any{
			{"role": "user", "content": buildPrompt(snap)},
		},
	})
	req, err := http.NewRequestWithContext(context.Background(), "POST", s.apiURL, bytes.NewReader(body))
	if err != nil {
		return "", err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("anthropic-version", "2023-06-01")
	switch auth.Method {
	case creds.MethodAPIKey:
		req.Header.Set("x-api-key", auth.APIKey)
	default: // MethodOAuth — beta header + Bearer are both required for OAuth-token inference
		req.Header.Set("Authorization", "Bearer "+auth.OAuth.AccessToken)
		req.Header.Set("anthropic-beta", anthropicBeta)
	}

	resp, err := s.client.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	raw, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("haiku api %d: %s", resp.StatusCode, string(raw))
	}
	return parseMessagesResponse(raw)
}

// parseMessagesResponse extracts the first text block from an Anthropic
// Messages response body. Bedrock's InvokeModel returns the same schema.
func parseMessagesResponse(raw []byte) (string, error) {
	var out struct {
		Content []struct {
			Type string `json:"type"`
			Text string `json:"text"`
		} `json:"content"`
	}
	if err := json.Unmarshal(raw, &out); err != nil {
		return "", err
	}
	for _, c := range out.Content {
		if c.Type == "text" {
			return strings.TrimSpace(c.Text), nil
		}
	}
	return "", errors.New("no text in response")
}

func buildPrompt(snap *transcript.Snapshot) string {
	var b strings.Builder
	b.WriteString("Summarise what this Claude Code session is currently doing, in ONE short sentence (max 12 words). Use present-continuous tense. No quotes, no prefix.\n\n")
	if snap.Slug != "" {
		b.WriteString("Conversation slug: ")
		b.WriteString(snap.Slug)
		b.WriteString("\n")
	}
	if t := truncate(snap.LastUserText, 400); t != "" {
		b.WriteString("Latest user prompt: ")
		b.WriteString(t)
		b.WriteString("\n")
	}
	if t := truncate(snap.LastAssistant, 400); t != "" {
		b.WriteString("Latest assistant text: ")
		b.WriteString(t)
		b.WriteString("\n")
	}
	if snap.LastToolUse != "" {
		b.WriteString("Latest tool used: ")
		b.WriteString(snap.LastToolUse)
		b.WriteString("\n")
	}
	return b.String()
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n] + "..."
}
