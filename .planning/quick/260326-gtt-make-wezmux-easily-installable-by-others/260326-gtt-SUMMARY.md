---
type: quick
task: 260326-gtt
description: Make Wezmux easily installable by others
completed: "2026-03-26"
duration: ~10 minutes
tasks_completed: 3
tasks_total: 3
files_created:
  - rust-toolchain.toml
  - .github/workflows/release.yml
files_modified:
  - Makefile
  - README.md
commits:
  - "807c4da: chore(260326-gtt): add rust-toolchain.toml and Makefile install/bundle targets"
  - "401ccd1: docs(260326-gtt): update README with complete install instructions"
  - "0a24704: chore(260326-gtt): add GitHub Actions release workflow for version tags"
---

# Quick Task 260326-gtt: Make Wezmux Easily Installable by Others

**One-liner:** Added rust-toolchain.toml pinning stable Rust, Makefile install/bundle targets with APP_DIR override, comprehensive README install section with prerequisites and --recursive clone, and a GitHub Actions release workflow that publishes Wezmux-macos.zip on v* tags.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Add rust-toolchain.toml and Makefile install target | 807c4da | rust-toolchain.toml, Makefile |
| 2 | Update README with complete install instructions | 401ccd1 | README.md |
| 3 | Add GitHub Actions release workflow | 0a24704 | .github/workflows/release.yml |

## What Was Done

### Task 1: rust-toolchain.toml and Makefile targets

Created `rust-toolchain.toml` at repo root pinning `channel = "stable"` with `clippy` and `rustfmt` components. This means any contributor with rustup installed gets the right toolchain automatically on first `cargo` invocation.

Added two new Makefile targets:
- `install` — builds release binaries, assembles bundle at `$(APP_DIR)` (default `/Applications/Wezmux.app`), copies Info.plist and terminal.icns, ad-hoc codesigns `wezterm-gui` binary only (never `--deep`)
- `bundle` — same as install but outputs to `target/Wezmux.app` for local iteration without touching `/Applications`

Also added `APP_DIR` variable at the top so users can override: `APP_DIR=~/Applications/Wezmux.app make install`.

### Task 2: README install section

Replaced the sparse manual-step install section with a structured flow:
- **Prerequisites**: rustup installation one-liner, `xcode-select --install` for C deps (harfbuzz/freetype/libpng/zlib from submodules), optional `gh` CLI for sidebar PR status
- **Clone**: `git clone --recursive` command, plus fallback `git submodule update --init --recursive` for those who already cloned
- **Build and install**: `make install` with note about custom `APP_DIR`
- **Development build**: `make bundle && open target/Wezmux.app`

### Task 3: GitHub Actions release workflow

Created `.github/workflows/release.yml` that triggers on `v*` tag pushes:
1. Checks out with `submodules: recursive`
2. Installs Rust stable via `dtolnay/rust-toolchain@stable`
3. Caches `~/.cargo/registry`, `~/.cargo/git`, and `target/` keyed on `Cargo.lock` hash
4. Builds release binaries for all four packages
5. Assembles `Wezmux.app` bundle and ad-hoc codesigns `wezterm-gui`
6. Creates `Wezmux-macos.zip` via `ditto` (preserves macOS extended attributes)
7. Publishes to GitHub Releases via `softprops/action-gh-release@v2` with `generate_release_notes: true`

## Deviations from Plan

None — plan executed exactly as written.

## Self-Check

Files created/modified:
- rust-toolchain.toml: EXISTS
- Makefile (install + bundle targets): EXISTS
- README.md (--recursive, xcode-select, make install, submodule update): EXISTS
- .github/workflows/release.yml (recursive, codesign, action-gh-release): EXISTS

Commits:
- 807c4da: EXISTS
- 401ccd1: EXISTS
- 0a24704: EXISTS

## Self-Check: PASSED
