package creds

import (
	"encoding/json"
	"errors"
	"fmt"
	"os/exec"
	"time"
)

type Creds struct {
	AccessToken  string
	RefreshToken string
	ExpiresAt    time.Time
}

// Expired returns true if the access token is past its expiry (with 1 min buffer).
func (c *Creds) Expired() bool {
	return time.Now().Add(time.Minute).After(c.ExpiresAt)
}

// LoadFromKeychain reads the Claude Code OAuth credentials from the macOS keychain.
// The entry is created when you log into `claude` and refreshed automatically by it,
// so a long-running process can just re-read on each call.
func LoadFromKeychain() (*Creds, error) {
	out, err := exec.Command("security", "find-generic-password",
		"-s", "Claude Code-credentials", "-w").Output()
	if err != nil {
		return nil, fmt.Errorf("security find-generic-password: %w", err)
	}
	var payload struct {
		ClaudeAiOauth struct {
			AccessToken  string `json:"accessToken"`
			RefreshToken string `json:"refreshToken"`
			ExpiresAt    int64  `json:"expiresAt"`
		} `json:"claudeAiOauth"`
	}
	if err := json.Unmarshal(out, &payload); err != nil {
		return nil, fmt.Errorf("parse keychain payload: %w", err)
	}
	if payload.ClaudeAiOauth.AccessToken == "" {
		return nil, errors.New("claudeAiOauth.accessToken empty — run `claude` to log in")
	}
	return &Creds{
		AccessToken:  payload.ClaudeAiOauth.AccessToken,
		RefreshToken: payload.ClaudeAiOauth.RefreshToken,
		ExpiresAt:    time.UnixMilli(payload.ClaudeAiOauth.ExpiresAt),
	}, nil
}
