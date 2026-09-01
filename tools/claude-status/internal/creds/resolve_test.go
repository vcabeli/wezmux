package creds

import "testing"

// clearAuthEnv resets the auth-related env vars so each test starts clean.
func clearAuthEnv(t *testing.T) {
	t.Setenv("CLAUDE_CODE_USE_BEDROCK", "")
	t.Setenv("ANTHROPIC_API_KEY", "")
	t.Setenv("ANTHROPIC_SMALL_FAST_MODEL", "")
}

func TestResolveBedrockWinsOverAPIKey(t *testing.T) {
	clearAuthEnv(t)
	t.Setenv("CLAUDE_CODE_USE_BEDROCK", "1")
	t.Setenv("ANTHROPIC_API_KEY", "sk-test")

	a, err := Resolve()
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if a.Method != MethodBedrock {
		t.Fatalf("Method = %v, want MethodBedrock", a.Method)
	}
}

func TestResolveBedrockDefaultModel(t *testing.T) {
	clearAuthEnv(t)
	t.Setenv("CLAUDE_CODE_USE_BEDROCK", "true")

	a, err := Resolve()
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if a.ModelID != defaultBedrockModel {
		t.Fatalf("ModelID = %q, want %q", a.ModelID, defaultBedrockModel)
	}
}

func TestResolveBedrockModelOverride(t *testing.T) {
	clearAuthEnv(t)
	t.Setenv("CLAUDE_CODE_USE_BEDROCK", "true")
	t.Setenv("ANTHROPIC_SMALL_FAST_MODEL", "us.anthropic.claude-haiku-4-5-20251001-v1:0")

	a, err := Resolve()
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if a.ModelID != "us.anthropic.claude-haiku-4-5-20251001-v1:0" {
		t.Fatalf("ModelID = %q, want override", a.ModelID)
	}
}

func TestResolveAPIKey(t *testing.T) {
	clearAuthEnv(t)
	t.Setenv("ANTHROPIC_API_KEY", "sk-test")

	a, err := Resolve()
	if err != nil {
		t.Fatalf("Resolve: %v", err)
	}
	if a.Method != MethodAPIKey {
		t.Fatalf("Method = %v, want MethodAPIKey", a.Method)
	}
	if a.APIKey != "sk-test" {
		t.Fatalf("APIKey = %q, want sk-test", a.APIKey)
	}
}

// Falsey Bedrock flags must fall through; with an API key set, that lands
// deterministically on MethodAPIKey without touching the keychain.
func TestResolveBedrockFalseyFallsThrough(t *testing.T) {
	for _, v := range []string{"0", "false", "FALSE", "no", ""} {
		t.Run("flag="+v, func(t *testing.T) {
			clearAuthEnv(t)
			t.Setenv("CLAUDE_CODE_USE_BEDROCK", v)
			t.Setenv("ANTHROPIC_API_KEY", "sk-test")

			a, err := Resolve()
			if err != nil {
				t.Fatalf("Resolve: %v", err)
			}
			if a.Method != MethodAPIKey {
				t.Fatalf("Method = %v, want MethodAPIKey (flag %q must be falsey)", a.Method, v)
			}
		})
	}
}

func TestTruthy(t *testing.T) {
	cases := map[string]bool{
		"1":     true,
		"true":  true,
		"TRUE":  true,
		"yes":   true,
		" 1 ":   true,
		"0":     false,
		"false": false,
		"False": false,
		"no":    false,
		"NO":    false,
		"":      false,
		"  ":    false,
	}
	for in, want := range cases {
		if got := truthy(in); got != want {
			t.Errorf("truthy(%q) = %v, want %v", in, got, want)
		}
	}
}
