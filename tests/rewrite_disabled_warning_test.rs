//! #508's `RTK_DISABLED` warning is stderr, so no rewrite test that compares
//! stdout can see it move: where it prints relative to the config's own
//! warnings, and which lines draw it at all, are pinned here.
//!
//! Unix only: the binary resolves its config directory through `$HOME`, and
//! only Unix takes that from the environment.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

const WARNING: &str = "RTK_DISABLED=1 detected";

/// Runs `rtk rewrite <cmd>` (one argv) against a home holding `config`, and
/// returns stderr.
fn stderr_of(home: &Path, cmd: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_rtk"))
        .args(["rewrite", cmd])
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local").join("share"))
        .env("RTK_DB_PATH", home.join("rtk.db"))
        .env("RTK_TELEMETRY_DISABLED", "1")
        .output()
        .expect("run rtk");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn home_with(config: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("tempdir");
    let dir = home.path().join(".config").join("rtk");
    std::fs::create_dir_all(&dir).expect("config dir");
    std::fs::write(dir.join("config.toml"), config).expect("config");
    home
}

#[test]
fn the_warning_prints_before_the_exclude_patterns_own_warnings() {
    let home = home_with("[hooks]\nexclude_commands = [\"^\", \"^(\", \"  \"]\n");
    let stderr = stderr_of(home.path(), "RTK_DISABLED=1 git status");
    let at_warning = stderr.find(WARNING).expect("the RTK_DISABLED warning");
    let at_pattern = stderr
        .find("exclude_commands pattern")
        .expect("a pattern warning");
    assert!(
        at_warning < at_pattern,
        "the RTK_DISABLED warning comes first: {stderr}"
    );
}

#[test]
fn the_patterns_warnings_still_print_for_a_line_with_nothing_to_rewrite() {
    let home = home_with("[hooks]\nexclude_commands = [\"^\"]\n");
    for cmd in ["", "   ", "echo hi", "echo a\necho b"] {
        let stderr = stderr_of(home.path(), cmd);
        assert!(
            stderr.contains("exclude_commands pattern"),
            "{cmd:?} compiles the config first: {stderr}"
        );
    }
}

#[test]
fn a_heredoc_in_the_text_as_written_silences_the_warning() {
    let home = home_with("");
    // `trim` strips the leading vertical tab, and `tokenize` glues it to the
    // fd number that follows: as written this is an `Arg` and a `<<`
    // redirect, so a heredoc; trimmed it is one `2<<` redirect. The warning
    // follows the text as written.
    let stderr = stderr_of(home.path(), "\x0b2<</dev/null; RTK_DISABLED=1 git status");
    assert!(!stderr.contains(WARNING), "{stderr}");
    let stderr = stderr_of(home.path(), "2<</dev/null; RTK_DISABLED=1 git status");
    assert!(stderr.contains(WARNING), "{stderr}");
    let stderr = stderr_of(home.path(), "RTK_DISABLED=1 git status");
    assert!(stderr.contains(WARNING), "{stderr}");
}
