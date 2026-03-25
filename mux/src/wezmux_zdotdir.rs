//! Creates a ZDOTDIR wrapper that ensures WEZMUX_BIN is prepended to PATH
//! after zsh's login initialization (which runs path_helper on macOS and
//! reorders PATH, pushing our bin/ directory to the end).
//!
//! We keep ZDOTDIR pointing at our wrapper for the entire zsh init sequence
//! so that all wrapper files (.zshenv, .zprofile, .zshrc, .zlogin) execute.
//! Each wrapper sources the corresponding user file from the real home dir.
//! The .zshrc wrapper re-prepends WEZMUX_BIN after path_helper has run.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Returns the path to the ZDOTDIR wrapper directory, creating it if needed.
pub fn ensure_zdotdir() -> anyhow::Result<PathBuf> {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    let dir = base.join(".cache").join("wezmux-zdotdir");

    fs::create_dir_all(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;

    let original_zdotdir = std::env::var("ZDOTDIR")
        .unwrap_or_else(|_| std::env::var("HOME").unwrap_or_default());

    write_zshenv(&dir, &original_zdotdir)?;
    write_zprofile(&dir, &original_zdotdir)?;
    write_zshrc(&dir, &original_zdotdir)?;
    write_zlogin(&dir, &original_zdotdir)?;

    Ok(dir)
}

fn write_zshenv(dir: &Path, original_zdotdir: &str) -> anyhow::Result<()> {
    // Store the real ZDOTDIR but do NOT reset ZDOTDIR yet — we need zsh
    // to keep reading from our wrapper directory for .zprofile/.zshrc/.zlogin.
    // Explicitly set HISTFILE so zsh doesn't default it to the wrapper dir.
    let content = format!(
        r#"# Wezmux ZDOTDIR wrapper
export _WEZMUX_REAL_ZDOTDIR="{original_zdotdir}"
export HISTFILE="${{HISTFILE:-$_WEZMUX_REAL_ZDOTDIR/.zsh_history}}"
if [[ -f "$_WEZMUX_REAL_ZDOTDIR/.zshenv" ]]; then
  ZDOTDIR="$_WEZMUX_REAL_ZDOTDIR" source "$_WEZMUX_REAL_ZDOTDIR/.zshenv"
fi
"#
    );
    fs::write(dir.join(".zshenv"), content)?;
    Ok(())
}

fn write_zprofile(dir: &Path, _original_zdotdir: &str) -> anyhow::Result<()> {
    let content = r#"# Wezmux ZDOTDIR wrapper
if [[ -f "$_WEZMUX_REAL_ZDOTDIR/.zprofile" ]]; then
  ZDOTDIR="$_WEZMUX_REAL_ZDOTDIR" source "$_WEZMUX_REAL_ZDOTDIR/.zprofile"
fi
"#;
    fs::write(dir.join(".zprofile"), content)?;
    Ok(())
}

fn write_zshrc(dir: &Path, _original_zdotdir: &str) -> anyhow::Result<()> {
    let content = r#"# Wezmux ZDOTDIR wrapper
if [[ -f "$_WEZMUX_REAL_ZDOTDIR/.zshrc" ]]; then
  ZDOTDIR="$_WEZMUX_REAL_ZDOTDIR" source "$_WEZMUX_REAL_ZDOTDIR/.zshrc"
fi

# Restore ZDOTDIR now that all init files have been sourced
export ZDOTDIR="$_WEZMUX_REAL_ZDOTDIR"

# Fix HISTFILE — zsh defaults it relative to ZDOTDIR at startup,
# which pointed at our wrapper dir. Reset it to the real location.
export HISTFILE="$ZDOTDIR/.zsh_history"

# Re-prepend WEZMUX_BIN to PATH after path_helper has reordered it
if [[ -n "${WEZMUX_BIN:-}" && -d "$WEZMUX_BIN" ]]; then
  export PATH="$WEZMUX_BIN:$PATH"
fi
unset _WEZMUX_REAL_ZDOTDIR
"#;
    fs::write(dir.join(".zshrc"), content)?;
    Ok(())
}

fn write_zlogin(dir: &Path, _original_zdotdir: &str) -> anyhow::Result<()> {
    let content = r#"# Wezmux ZDOTDIR wrapper
if [[ -f "${_WEZMUX_REAL_ZDOTDIR:-$HOME}/.zlogin" ]]; then
  source "${_WEZMUX_REAL_ZDOTDIR:-$HOME}/.zlogin"
fi
"#;
    fs::write(dir.join(".zlogin"), content)?;
    Ok(())
}
