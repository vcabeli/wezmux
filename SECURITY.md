# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Wezmux, please report it responsibly:

1. **Do not** open a public issue.
2. Email the maintainer or use [GitHub's private vulnerability reporting](https://github.com/vcabeli/wezmux/security/advisories/new).

You should receive a response within 7 days.

## Scope

Wezmux is a terminal emulator that executes arbitrary commands on your behalf.
Security issues in the terminal emulation layer (escape sequence injection,
privilege escalation via OSC handling, etc.) are in scope.

Issues that require the user to run malicious commands are generally out of scope
— the terminal is designed to run whatever you tell it to.
