# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> Lives in the wezmux monorepo at `tools/claude-status` (a standalone Go module,
> outside the Cargo workspace). Forked from `claude-tower`, which sourced its
> live feed from `cmux events`; cmux has been removed entirely. Sessions are fed
> by a Claude Code hook (`claude-status hook`) that writes one state file per
> session, which the dashboard reads. This replaced an earlier ps/argv approach
> that failed whenever the session id wasn't in the command line (alias launches,
> bare `--resume`/`--continue`, worktrees).

## Commands

```bash
go build -o claude-status .                  # produces ./claude-status
go vet ./...                                 # static checks
go test ./...                                # run all tests
go test ./internal/registry -run TestReconcile   # single test pattern
./claude-status                              # run the TUI
```

**macOS only** — depends on the `security` CLI to read the keychain. Session discovery is via Claude Code hooks (see Architecture), so there's no cmux and no `ps` scraping.

## Architecture

Two modes of the same binary. As a **hook** (`claude-status hook`), each Claude Code event writes a state file. As the **dashboard** (no args), a read loop reconciles those files into a registry and a Bubble Tea program renders it.

```
claude (any launch)
   │  hooks fire: UserPromptSubmit / Pre|PostToolUse / Notification / Stop / SessionStart|End
   ▼
`claude-status hook`  ──►  feed.RunHook  ──►  ~/.cache/claude-status/sessions/<id>.json
                                                          │
   dashboard read loop (every 1s)                         │
        │                                                 ▼
        ▼                                          feed.Load()
registry.Reconcile(states)  ──►  returns sessions whose state advanced
        │                                         │
        ▼                                         ▼
  (session state)                    summarizer.Request (debounced 5s)
                                                  │
                                                  ▼
                            transcript.TailPath(state.TranscriptPath) + creds.Resolve
                                                  │
                                                  ▼
                                           Haiku API call ──► registry.SetSummary (callback)

ui.Model ticks every 1s and renders from registry.Snapshot()
```

Key invariants when changing this pipeline:

- **`main` dispatches hook mode first.** `os.Args[1] == "hook"` runs `feed.RunHook(os.Stdin)` and returns *before* any TUI setup. Keep it fast and side-effect-free beyond the state-file write; it runs on every hook of every session.
- **`feed.RunHook` must never hard-fail.** A panicking/erroring hook would disrupt the Claude session. It swallows parse/IO errors and no-ops on missing `session_id`. State files are written atomically (temp + rename) since many sessions write concurrently — one file per `session_id`, so no cross-session contention.
- **The registry is the synchronization point.** The read loop and each `SetSummary` callback mutate `registry` from outside the `tea.Program` goroutine; `registry`'s own `sync.RWMutex` makes that safe, and the UI only ever *reads* via `Snapshot()`. Don't block in `Update`.
- **`summarizer.Request` is debounced per-session (5s)** and tails `state.TranscriptPath` (the hook-reported path) via `transcript.TailPath` — **not** `Tail(cwd, sid)`. The cwd→path encoding (`encodeCwd`, `/`→`-`) is lossy for worktrees and dotted paths, so it's only a fallback; the reported path is authoritative.

## Non-obvious constraints

- **Status comes straight from hook events**, mapped in `feed.statusFor`: prompt/tool events → `running`; `PreToolUse` for `AskUserQuestion` and any `Notification` → `awaiting`; `Stop`/`SessionStart` → `idle`; `SessionEnd` → file removed. Events not in the map (e.g. `SubagentStop`) preserve the previous status. `registry.statusFromFeed` maps the string to the enum.
- **Hooks merge across settings sources.** A wrapper that launches `claude --settings '{…hooks…}'` (e.g. wezmux) does *not* suppress the user's `~/.claude/settings.json` hooks — Claude Code runs hooks from all sources (identical command/url deduped). So claude-status's hook fires even inside wezmux. Edited settings apply only to *newly started* sessions.
- **A session missing from the feed is marked ended.** `Reconcile` diffs `feed.Load()` against the known set; anything absent gets `StatusEnded`. Absence happens two ways: `SessionEnd` removed the file (graceful), or `feed.Load` dropped it as stale (`feed.StaleAfter`, 12h — the crash backstop, since a killed session fires no `SessionEnd`). Ended sessions stay in the map but are filtered out of `Snapshot`, so registry size ≠ displayed row count.
- **Three auth methods, resolved by `creds.Resolve()`** with Claude Code's own env precedence: `CLAUDE_CODE_USE_BEDROCK` truthy → Bedrock; else `ANTHROPIC_API_KEY` non-empty → x-api-key; else keychain OAuth. The OAuth path reads `Claude Code-credentials` from the macOS keychain on every call (so token refreshes from a parallel `claude` login are picked up automatically) and requires two non-defaults in `summarizer.callMessages`: the `anthropic-beta: oauth-2025-04-20` header AND a `system` prompt that begins with the literal Claude Code identity string (`claudeCodeIdent` constant). Removing either breaks OAuth auth specifically — they're tied to how Anthropic gates OAuth-token inference (the API-key path needs neither, but shares the body builder).
- **Bedrock requests differ in shape, not schema.** `summarizer.callBedrock` puts `anthropic_version: "bedrock-2023-05-31"` in the body and **no `model` field** — the model id is the `InvokeModel` `ModelId` param, defaulting to `eu.anthropic.claude-haiku-4-5-20251001-v1:0` (override via `ANTHROPIC_SMALL_FAST_MODEL`). The response body is the same Messages schema, parsed by the shared `parseMessagesResponse`. The Bedrock client is built lazily via `sync.Once` (`config.LoadDefaultConfig` walks the full AWS credential chain) and a load error is cached for the process lifetime.

## Fallback behaviour

Auth failures degrade, they don't error. `summarizer.summarize` distinguishes two stages: *resolve-stage* failures (keychain missing, OAuth token expired, AWS `LoadDefaultConfig` error) return a humanized version of the transcript's `slug` field with a nil error, while *call-stage* failures (HTTP non-200, `InvokeModel` errors, response-parse errors) propagate as `Result.Err`. This is intentional — the UI should keep working without auth — so any new auth/setup paths should preserve a non-empty fallback string rather than surfacing the error to the user.
