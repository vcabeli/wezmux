#!/bin/bash
# wezmux hook: Codex UserPromptSubmit event
# Emits OSC 7777 structured status AND OSC 9 notification to terminal pane.

printf '\033]7777;status;working\007' > /dev/tty 2>/dev/null || true
printf '\033]9;Codex is working...\007' > /dev/tty 2>/dev/null || true
exit 0
