# Phase 1: Fork Setup - Context

**Gathered:** 2026-03-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Fork WezTerm from upstream `main` branch, configure upstream remote for future rebasing, and verify the project builds and runs correctly on macOS. No code modifications — this phase produces a clean, buildable starting point.

</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion

User deferred all setup decisions — "do whatever you think is best, pretty straightforward." The following are Claude's recommended defaults:

- **D-01:** GitHub fork (not bare clone) — preserves the option to submit upstream PRs and gives a familiar GitHub-hosted remote. Fork `wez/wezterm` to user's GitHub account.
- **D-02:** Work on `main` branch initially. Create a `wezmux/dev` branch for custom work only when Phase 2 begins (first actual code change). Keep `main` as the upstream tracking branch for clean `git fetch upstream && git rebase upstream/main`.
- **D-03:** Keep WezTerm naming for now — no rename in Phase 1. Rebranding (binary name, app bundle, window title) is a separate concern that can happen in a later phase or as a quick task once the sidebar exists. Avoids touching build config prematurely.
- **D-04:** Use `cargo build` (debug) for daily development. Only build release for final testing. Target `--package wezterm` for faster iteration (skip ancillary tools like `wezterm-mux-server` unless needed).
- **D-05:** Fork from `main` HEAD, not the `20240203` tagged release — per CLAUDE.md guidance, main has ~14 months of additional commits including wgpu 25, tokio 1.43, and render fixes.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

No external specs — requirements fully captured in decisions above. The fork target is the upstream WezTerm repository.

### Upstream
- `https://github.com/wez/wezterm` — upstream repository to fork from (main branch HEAD)

### Project
- `.planning/PROJECT.md` — project vision, constraints, key decisions
- `.planning/REQUIREMENTS.md` — FORK-01, FORK-02, FORK-03 acceptance criteria
- `CLAUDE.md` — technology stack, build commands, "What NOT to Use" guidance

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- No existing code — the fork does not yet exist. This phase creates it.

### Established Patterns
- WezTerm uses `cargo build --package wezterm` for the GUI binary
- `RUST_LOG=wezterm_gui=debug` for runtime logging
- `cargo clippy --package wezterm-gui -- -D warnings` for linting

### Integration Points
- The fork will be the foundation for all subsequent phases
- Upstream remote (`upstream`) must be configured for periodic rebasing

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. User confirmed this is straightforward setup work.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 01-fork-setup*
*Context gathered: 2026-03-24*
