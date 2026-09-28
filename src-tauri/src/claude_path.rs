//! Makes the `claude` CLI reachable through `PATH` for every spawn c9watch makes.
//!
//! A macOS app started from Finder, the Dock or a login item inherits launchd's
//! minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`), which contains none of the
//! places Claude Code installs to. Without this, the `claude agents --json`
//! probe fails and c9watch silently falls back to the legacy process scanner.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

const CLAUDE_EXE: &str = if cfg!(windows) {
    "claude.exe"
} else {
    "claude"
};

/// Appends the first well-known Claude Code install directory to `PATH` when
/// `claude` isn't already reachable through it.
///
/// Must run before any other thread starts, since it mutates the process
/// environment.
pub fn ensure_claude_on_path() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    if let Some(updated) = augmented_path(&current, &home, |p| p.is_file()) {
        std::env::set_var("PATH", updated);
    }
}

/// Install locations in the order Claude Code's own installers use them:
/// native installer, legacy local install, Homebrew (Apple silicon, Intel).
fn candidate_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local").join("bin"),
        home.join(".claude").join("local"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
    ]
}

fn augmented_path(
    current: &OsStr,
    home: &Path,
    is_file: impl Fn(&Path) -> bool,
) -> Option<OsString> {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(current).collect();
    if dirs.iter().any(|d| is_file(&d.join(CLAUDE_EXE))) {
        return None;
    }
    let found = candidate_dirs(home)
        .into_iter()
        .find(|d| is_file(&d.join(CLAUDE_EXE)))?;
    dirs.push(found);
    std::env::join_paths(dirs).ok()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn exists_in(present: &'static [&'static str]) -> impl Fn(&Path) -> bool {
        move |p| present.iter().any(|x| Path::new(x) == p)
    }

    #[test]
    fn leaves_path_alone_when_claude_is_reachable() {
        let path = OsStr::new("/usr/bin:/opt/homebrew/bin");
        let is_file = exists_in(&["/opt/homebrew/bin/claude", "/h/.local/bin/claude"]);
        assert_eq!(augmented_path(path, Path::new("/h"), is_file), None);
    }

    #[test]
    fn appends_native_install_dir_to_launchd_path() {
        let path = OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin");
        let is_file = exists_in(&["/h/.local/bin/claude", "/opt/homebrew/bin/claude"]);
        assert_eq!(
            augmented_path(path, Path::new("/h"), is_file),
            Some(OsString::from(
                "/usr/bin:/bin:/usr/sbin:/sbin:/h/.local/bin"
            ))
        );
    }

    #[test]
    fn falls_back_to_homebrew_when_no_home_install() {
        let path = OsStr::new("/usr/bin:/bin");
        let is_file = exists_in(&["/opt/homebrew/bin/claude"]);
        assert_eq!(
            augmented_path(path, Path::new("/h"), is_file),
            Some(OsString::from("/usr/bin:/bin:/opt/homebrew/bin"))
        );
    }

    #[test]
    fn no_change_when_claude_is_not_installed_anywhere() {
        let path = OsStr::new("/usr/bin:/bin");
        assert_eq!(augmented_path(path, Path::new("/h"), exists_in(&[])), None);
    }
}
