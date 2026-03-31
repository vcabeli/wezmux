#!/bin/bash
# Install wezmux Codex hooks
# Merges into existing ~/.codex/hooks.json (preserves other hooks like cmux)
# and enables codex_hooks in ~/.codex/config.toml
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK_DIR="$SCRIPT_DIR/hooks/codex"
CODEX_DIR="$HOME/.codex"

mkdir -p "$CODEX_DIR"

# --- 1. Merge hooks into hooks.json ---
HOOKS_JSON="$CODEX_DIR/hooks.json"

# Our hook entries as JSON fragments
WEZMUX_PROMPT_SUBMIT='{"hooks":[{"type":"command","command":"'"$HOOK_DIR"'/on-prompt-submit.sh","timeout":5}]}'
WEZMUX_STOP='{"hooks":[{"type":"command","command":"'"$HOOK_DIR"'/on-stop.sh","timeout":5}]}'
WEZMUX_PRE_TOOL='{"hooks":[{"type":"command","command":"'"$HOOK_DIR"'/on-pre-tool-use.sh","timeout":5}]}'

is_wezmux_hook() {
    echo "$1" | jq -e '.. | .command? // empty | test("wezmux|on-prompt-submit\\.sh|on-stop\\.sh|on-pre-tool-use\\.sh")' >/dev/null 2>&1
}

if ! command -v jq >/dev/null 2>&1; then
    echo "ERROR: jq is required to safely merge Codex hooks."
    echo "Install with: brew install jq"
    exit 1
fi

if [ -f "$HOOKS_JSON" ] && [ -s "$HOOKS_JSON" ]; then
    # Existing hooks.json — merge our hooks in, replacing any previous wezmux entries

    # For each event type, filter out old wezmux entries then append ours
    MERGED=$(jq \
        --argjson prompt "$WEZMUX_PROMPT_SUBMIT" \
        --argjson stop "$WEZMUX_STOP" \
        --argjson tool "$WEZMUX_PRE_TOOL" \
        '
        # Helper: keep entries whose commands do not match wezmux paths
        def remove_wezmux:
            [ .[]? | select(
                (.hooks // []) | all(.command | test("wezmux|on-prompt-submit\\.sh|on-stop\\.sh|on-pre-tool-use\\.sh") | not)
            ) ];

        .hooks.UserPromptSubmit = ((.hooks.UserPromptSubmit // []) | remove_wezmux) + [$prompt] |
        .hooks.Stop = ((.hooks.Stop // []) | remove_wezmux) + [$stop] |
        .hooks.PreToolUse = ((.hooks.PreToolUse // []) | remove_wezmux) + [$tool]
        ' "$HOOKS_JSON")

    echo "$MERGED" | jq '.' > "$HOOKS_JSON"
    echo "Merged wezmux hooks into $HOOKS_JSON (existing hooks preserved)"
else
    # No existing hooks.json — create fresh
    jq -n \
        --argjson prompt "$WEZMUX_PROMPT_SUBMIT" \
        --argjson stop "$WEZMUX_STOP" \
        --argjson tool "$WEZMUX_PRE_TOOL" \
        '{
            hooks: {
                UserPromptSubmit: [$prompt],
                Stop: [$stop],
                PreToolUse: [$tool]
            }
        }' > "$HOOKS_JSON"
    echo "Created $HOOKS_JSON"
fi

# --- 2. Enable codex_hooks in config.toml ---
CONFIG_TOML="$CODEX_DIR/config.toml"

if [ -f "$CONFIG_TOML" ]; then
    if grep -q 'codex_hooks' "$CONFIG_TOML"; then
        sed -i '' 's/codex_hooks *= *false/codex_hooks = true/' "$CONFIG_TOML"
        echo "Enabled codex_hooks in $CONFIG_TOML"
    elif grep -q '\[features\]' "$CONFIG_TOML"; then
        sed -i '' '/\[features\]/a\
codex_hooks = true
' "$CONFIG_TOML"
        echo "Added codex_hooks = true to [features] in $CONFIG_TOML"
    else
        echo "" >> "$CONFIG_TOML"
        echo "[features]" >> "$CONFIG_TOML"
        echo "codex_hooks = true" >> "$CONFIG_TOML"
        echo "Added [features] section with codex_hooks = true to $CONFIG_TOML"
    fi
else
    cat > "$CONFIG_TOML" <<'EOF'
[features]
codex_hooks = true
EOF
    echo "Created $CONFIG_TOML with codex_hooks = true"
fi

# --- 3. Make hook scripts executable ---
chmod +x "$HOOK_DIR"/*.sh
echo ""
echo "Wezmux Codex hooks installed successfully."
echo "Restart Codex for hooks to take effect."
