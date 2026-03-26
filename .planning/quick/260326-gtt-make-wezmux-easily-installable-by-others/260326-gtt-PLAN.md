---
type: quick
description: Make Wezmux easily installable by others
autonomous: true
files_modified:
  - README.md
  - Makefile
  - rust-toolchain.toml
  - .github/workflows/release.yml
---

<objective>
Make Wezmux installable by someone who clones the repo for the first time. Currently, the README omits --recursive for submodules, there is no rust-toolchain.toml, no `make install` target, and no CI for release builds.

Purpose: Lower the barrier from "read the source to figure out how to build" to "clone, make install, done."
Output: Updated README, rust-toolchain.toml, Makefile install target, GitHub Actions release workflow.
</objective>

<execution_context>
@.planning/quick/260326-gtt-make-wezmux-easily-installable-by-others/260326-gtt-PLAN.md
</execution_context>

<context>
@README.md
@Makefile
@assets/macos/WezTerm.app/Contents/Info.plist
@ci/check-rust-version.sh
@.gitmodules
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add rust-toolchain.toml and Makefile install target</name>
  <files>rust-toolchain.toml, Makefile</files>
  <action>
1. Create `rust-toolchain.toml` at repo root:
   - `[toolchain]` section with `channel = "stable"` and `components = ["clippy", "rustfmt"]`
   - This pins the toolchain so contributors get the right Rust version automatically via rustup.

2. Add an `install` target to the Makefile that does the full build-sign-bundle flow:
   - `make install` should:
     a. `cargo build --release -p wezterm-gui -p wezterm -p wezterm-mux-server -p strip-ansi-escapes`
     b. Create `/Applications/Wezmux.app/Contents/MacOS/` directory (mkdir -p)
     c. Copy the release binaries (`wezterm-gui`, `wezterm`, `wezterm-mux-server`, `strip-ansi-escapes`) into that MacOS dir
     d. Copy `assets/macos/WezTerm.app/Contents/Info.plist` to `/Applications/Wezmux.app/Contents/Info.plist`
     e. Copy `assets/macos/WezTerm.app/Contents/Resources/terminal.icns` to `/Applications/Wezmux.app/Contents/Resources/terminal.icns` (mkdir -p Resources first)
     f. Run `codesign --force --sign - /Applications/Wezmux.app/Contents/MacOS/wezterm-gui` (ad-hoc sign the main binary only, never --deep on the bundle)
     g. Print a success message: "Wezmux.app installed to /Applications/Wezmux.app"
   - Also add an `APP_DIR` variable at the top defaulting to `/Applications/Wezmux.app` so users can override with `make install APP_DIR=~/Applications/Wezmux.app`
   - Add `install` to the `.PHONY` list
   - Add a `bundle` target that does the same but to `target/Wezmux.app` (local, no /Applications copy) for development use. Add to `.PHONY`.
  </action>
  <verify>
    <automated>cd /Users/vincentcabeli/code/wezmux && cat rust-toolchain.toml && grep -q 'install' Makefile && grep -q 'bundle' Makefile && grep -q 'APP_DIR' Makefile</automated>
  </verify>
  <done>rust-toolchain.toml exists with stable channel. Makefile has install and bundle targets that handle build, codesign, and bundle creation.</done>
</task>

<task type="auto">
  <name>Task 2: Update README with complete install instructions and prerequisites</name>
  <files>README.md</files>
  <action>
Replace the "## Install" section in README.md with comprehensive instructions:

1. **Prerequisites** subsection listing:
   - Rust toolchain (installed automatically via rust-toolchain.toml, or manual: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
   - Xcode Command Line Tools: `xcode-select --install` (needed for C dependencies: harfbuzz, freetype, libpng, zlib)
   - Optional: `gh` CLI for PR status in sidebar (`brew install gh`)

2. **Clone** step — MUST include `--recursive`:
   ```
   git clone --recursive https://github.com/vcabeli/wezmux.git
   cd wezmux
   ```
   Add a note: "If you already cloned without --recursive, run `git submodule update --init --recursive`"

3. **Build and install** step using the new Makefile target:
   ```
   make install
   ```
   Note it installs to /Applications/Wezmux.app by default. Mention `APP_DIR=~/Applications/Wezmux.app make install` for custom location.

4. **Development build** step:
   ```
   make bundle
   open target/Wezmux.app
   ```

Keep the existing Config and Credits sections unchanged.
  </action>
  <verify>
    <automated>cd /Users/vincentcabeli/code/wezmux && grep -q 'recursive' README.md && grep -q 'xcode-select' README.md && grep -q 'make install' README.md && grep -q 'submodule update' README.md</automated>
  </verify>
  <done>README Install section has prerequisites (Rust, Xcode CLT, optional gh), --recursive clone command, fallback submodule init command, and make install / make bundle instructions.</done>
</task>

<task type="auto">
  <name>Task 3: Add GitHub Actions release workflow</name>
  <files>.github/workflows/release.yml</files>
  <action>
Create `.github/workflows/release.yml` that builds a release .app bundle on tag push:

Trigger: `on: push: tags: ['v*']`

Single job `build-macos` running on `macos-latest`:

Steps:
1. `actions/checkout@v4` with `submodules: recursive`
2. Install Rust stable via `dtolnay/rust-toolchain@stable`
3. Cache cargo registry and target dir via `actions/cache@v4` with key based on `Cargo.lock` hash
4. `cargo build --release -p wezterm-gui -p wezterm -p wezterm-mux-server -p strip-ansi-escapes`
5. Assemble app bundle in a script step:
   - mkdir -p Wezmux.app/Contents/MacOS Wezmux.app/Contents/Resources
   - cp target/release/wezterm-gui Wezmux.app/Contents/MacOS/
   - cp target/release/wezterm Wezmux.app/Contents/MacOS/
   - cp target/release/wezterm-mux-server Wezmux.app/Contents/MacOS/
   - cp target/release/strip-ansi-escapes Wezmux.app/Contents/MacOS/
   - cp assets/macos/WezTerm.app/Contents/Info.plist Wezmux.app/Contents/
   - cp assets/macos/WezTerm.app/Contents/Resources/terminal.icns Wezmux.app/Contents/Resources/
   - codesign --force --sign - Wezmux.app/Contents/MacOS/wezterm-gui
6. Create zip: `ditto -c -k --keepParent Wezmux.app Wezmux-macos.zip` (ditto preserves macOS metadata better than zip)
7. Create GitHub release using `softprops/action-gh-release@v2` with `files: Wezmux-macos.zip` and `generate_release_notes: true`

Add a comment at the top of the file: "# Builds and releases Wezmux.app bundle on version tags (v*)"
  </action>
  <verify>
    <automated>cd /Users/vincentcabeli/code/wezmux && cat .github/workflows/release.yml | head -5 && grep -q 'recursive' .github/workflows/release.yml && grep -q 'codesign' .github/workflows/release.yml && grep -q 'action-gh-release' .github/workflows/release.yml</automated>
  </verify>
  <done>GitHub Actions workflow exists that triggers on version tags, builds release binaries, assembles and codesigns an app bundle, and publishes it as a GitHub Release with a .zip artifact.</done>
</task>

</tasks>

<verification>
- `cat rust-toolchain.toml` shows stable channel
- `grep -c 'install\|bundle' Makefile` shows both targets present
- README.md contains --recursive, prerequisites, make install instructions
- `.github/workflows/release.yml` is valid YAML with tag trigger and release upload
</verification>

<success_criteria>
A new contributor can follow README from zero to running Wezmux.app. Pushing a version tag produces a downloadable .app bundle via GitHub Releases.
</success_criteria>
