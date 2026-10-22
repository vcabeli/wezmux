package creds

import (
	"os"
	"strings"
)

// Method selects how the summarizer authenticates against the model API.
type Method int

const (
	MethodOAuth   Method = iota // keychain Bearer token + oauth beta header
	MethodAPIKey                // x-api-key header
	MethodBedrock               // AWS Bedrock InvokeModel (SigV4 via SDK)
)

const defaultBedrockModel = "eu.anthropic.claude-haiku-4-5-20251001-v1:0"

// Auth is the resolved auth method plus its payload. Exactly one payload
// field is meaningful per Method.
type Auth struct {
	Method  Method
	OAuth   *Creds // MethodOAuth: token (may be expired — caller checks)
	APIKey  string // MethodAPIKey
	ModelID string // MethodBedrock: Bedrock model / inference-profile id
}

// Resolve picks the auth method from env, mirroring Claude Code's own
// conventions and precedence:
//
//	CLAUDE_CODE_USE_BEDROCK truthy -> Bedrock
//	ANTHROPIC_API_KEY non-empty    -> API key
//	else                           -> keychain OAuth
//
// The Bedrock branch does no AWS work here — client setup is deferred to the
// summarizer so a missing AWS config degrades to the slug fallback there.
func Resolve() (*Auth, error) {
	if truthy(os.Getenv("CLAUDE_CODE_USE_BEDROCK")) {
		modelID := os.Getenv("ANTHROPIC_SMALL_FAST_MODEL")
		if modelID == "" {
			modelID = defaultBedrockModel
		}
		return &Auth{Method: MethodBedrock, ModelID: modelID}, nil
	}
	if key := os.Getenv("ANTHROPIC_API_KEY"); key != "" {
		return &Auth{Method: MethodAPIKey, APIKey: key}, nil
	}
	c, err := LoadFromKeychain()
	if err != nil {
		return nil, err
	}
	return &Auth{Method: MethodOAuth, OAuth: c}, nil
}

// truthy mirrors Claude Code's env-flag parsing: set and not an explicit
// "off" value means on.
func truthy(v string) bool {
	switch strings.ToLower(strings.TrimSpace(v)) {
	case "", "0", "false", "no":
		return false
	}
	return true
}
