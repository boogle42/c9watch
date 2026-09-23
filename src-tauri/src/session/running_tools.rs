//! Process-tree evidence that a pending tool call is executing rather than
//! waiting on a permission prompt. Claude Code runs each Bash call as a child
//! shell (`<shell> -c ... eval '<command>'`) of the `claude` process, and only
//! after the call is approved, so a live matching child means "running".
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Whether `parent_pid` has a live child process whose command line runs
/// `command`.
pub fn bash_command_running(parent_pid: u32, command: &str) -> bool {
    if command.trim().is_empty() {
        return false;
    }
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new().with_cmd(UpdateKind::Always),
    );
    let parent = Pid::from_u32(parent_pid);
    system
        .processes()
        .values()
        .filter(|process| process.parent() == Some(parent))
        .any(|process| {
            let cmdline = process
                .cmd()
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            cmdline_runs_command(&cmdline, command)
        })
}

/// Compares with quotes and backslashes removed, since the shell wrapper
/// single-quotes the command and escapes the quotes inside it (`'"'"'`, `'\''`).
fn cmdline_runs_command(cmdline: &str, command: &str) -> bool {
    let normalize = |s: &str| {
        s.chars()
            .filter(|c| !matches!(c, '\'' | '"' | '\\'))
            .collect::<String>()
    };
    normalize(cmdline).contains(&normalize(command))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_quoted_eval_wrapper() {
        let command =
            r#"cd ~/repo && ./tests/run.sh > /tmp/log 2>&1; echo "exit $?"; grep -v '^remote: *$'"#;
        for escaped_quote in [r#"'"'"'"#, r"'\''"] {
            let cmdline = format!(
                "/bin/zsh -c source ~/.claude/shell-snapshots/snapshot.sh && eval '{}' < /dev/null && pwd -P >| /tmp/cwd",
                command.replace('\'', escaped_quote)
            );
            assert!(cmdline_runs_command(&cmdline, command), "{cmdline}");
            assert!(!cmdline_runs_command(&cmdline, "cd ~/repo && make"));
        }
    }

    #[test]
    fn empty_command_never_matches() {
        assert!(!bash_command_running(std::process::id(), "  "));
    }

    #[test]
    fn finds_a_real_child_shell() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 5; echo c9watch-running-tools-test"])
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let found = bash_command_running(std::process::id(), "echo c9watch-running-tools-test");
        let missing = bash_command_running(std::process::id(), "echo something-else");
        child.kill().unwrap();
        let _ = child.wait();
        assert!(found);
        assert!(!missing);
    }
}
