# Wezmux v1.0 Release Audit

Generated: 2026-04-03
Source: Independent review by 5 specialist agents (onboarding, release engineering, code quality, upstream references, open source licensing).

## Blockers

- [x] **`make install` silently modifies `~/.codex/hooks.json`** -- modifying user dotfiles without consent during install is a trust violation. Make it opt-in or a separate `make install-codex-hooks` target.
- [x] **LICENSE.md missing fork copyright** -- new Wezmux code has no stated copyright holder. Add `Copyright (c) 2025-Present Vincent Cabeli` alongside the existing Wez Furlong line.
- [x] **`gen_macos*.yml` CI workflows are dead** -- `github.repository == 'wezterm/wezterm'` guard prevents all uploads; artifact globs look for `WezTerm-*.zip` but `deploy.sh` produces `Wezmux-*.zip`. Either fix or delete these and rely on `release.yml`.
- [x] **Failing unit test in notification store** -- `mux/src/notification.rs` test `unread_counts_and_mark_read_work` expects unread count 2, but dedup logic correctly reduces it to 1. Update test expectations.
- [x] **Stale upstream links actively misdirect users** -- `build-problem.md` links to wezterm.org; `docs/faq.md` sends bug reports to `wezterm/wezterm`; `PRIVACY.md` says binaries come from wezterm.org; `docs/faq.md` downloads terminfo from upstream repo.

## Should Fix

- [x] **Issue templates list non-macOS platforms** -- `bug.yml` OS dropdown includes Windows, Linux, FreeBSD. Trim to macOS for v1.0. Log paths reference Linux/Windows locations, not macOS.
- [x] **`release.yml` builds single-arch only** -- no Intel (x86_64) binary. Bundle also missing `shell-integration`, `shell-completion`, `terminfo`, and `bin/` compared to what `deploy.sh` and `make install` produce.
- [x] **Agent message data not sanitized** -- `mux/src/lib.rs` stores OSC 7777 message data without stripping control characters (unlike notification text which goes through `sanitize_notification_text()`).
- [x] **Agent status store leaks on pane removal** -- `MuxNotification::PaneRemoved` cleans up notification store but not `agent_status_store`. `SidebarState::last_known_agents` also never cleaned on pane removal. Unbounded growth over long sessions.
- [x] **`hex_to_linear` panics on short input** -- `render/sidebar.rs` slices `hex[0..2]` etc. without bounds check. `unwrap_or` handles parse errors but slice indexing panics if string is under 6 chars.
- [x] **Product name "WezTerm" in user-facing docs** -- `PRIVACY.md`, `sponsor.md`, `CONTRIBUTING.md`, `what-is-a-terminal.md`, `scrollback.md`, `config/appearance.md`, `config/fonts.md`, `config/keyboard-concepts.md` all present the product as WezTerm.
- [x] **No CODE_OF_CONDUCT.md or SECURITY.md** -- expected for a public v1.0 of a terminal emulator.
- [x] **`config.md` missing Wezmux keyboard shortcuts** -- `Option+K/J`, `Option+1..9`, `Option+U`, `Cmd+B` are in the README but not in the config reference.
- [x] **No workspace-level Cargo version** -- `Info.plist` says `1.0.0` but no `Cargo.toml` declares a matching version. No single source of truth.
- [x] **Hook scripts don't strip semicolons** -- semicolons delimit OSC parameters. Low risk (parser ignores extra segments) but incomplete sanitization in `on-notification.sh` and `on-stop.sh`.
- [ ] **Binary still called `wezterm-gui`** -- process name in Activity Monitor / `ps` shows `wezterm-gui`. Potential collision if stock WezTerm is also installed. Consider renaming to `wezmux-gui`.
- [x] **CONTRIBUTING.md is stock WezTerm** -- says "the `src` directory holds the code for the `wezterm` program", references `gdb` instead of `lldb`, no mention of sidebar/notification/agent code locations or `Makefile` targets.
- [x] **Sub-crate LICENSE files** -- `mux/` and `wezterm-gui/` contain substantial new Wezmux code but LICENSE files only credit Wez Furlong (and `wezterm-gui/` has no LICENSE at all).
- [x] **`ci/check-rust-version.sh`** -- minimum version is `1.71.0` but project needs `1.75+` for wgpu 25. Error message references `wezterm.org`.
- [x] **`no-response.yml` workflow is dead** -- gated on `github.repository == 'wezterm/wezterm'`.
- [x] **`make bundle` inconsistent with `make install`** -- missing `chmod +x` on `bin/hooks/codex/*.sh`.

## Nice to Have (post-v1.0)

- [ ] Changelog placeholder -- `docs/changelog.md` has empty `### 1.0.0` section, needs actual release notes.
- [ ] README "Why?" section tone -- casual for a public release, could briefly explain the actual problem being solved.
- [ ] README build time estimate -- first-time build compiles hundreds of crates, takes 10-20 min.
- [ ] GitHub badge row (build status, license, version).
- [ ] `docs/index.md` hero screenshots may show stock WezTerm, not the Wezmux sidebar.
- [ ] Inherited WezTerm screenshots in `docs/screenshots/` add clutter.
- [ ] Per-file license headers on new Wezmux source files (upstream doesn't use them either, so no inconsistency).
- [ ] Notification dedup uses linear scan of VecDeque (up to 1000 entries). Fine in practice.
- [ ] `mark_pane_read` does N separate map lookups instead of a single set-to-zero.
- [ ] `assets/macos/WezTerm.app` source directory still named after upstream (cosmetic, .app is correctly branded at runtime).
- [ ] `lock.yml` workflow unnecessary for a personal project.
- [ ] `ci/update-doc-versions.py` has hardcoded upstream nightly version.
- [ ] Trademark: "Wezmux" contains "Wez" -- low risk but worth a courtesy check with Wez Furlong.
- [ ] Credits section mentions specific model names (Opus 4.6, GPT 5.4) -- potentially distracting.
