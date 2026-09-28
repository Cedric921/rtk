//! `trim` also strips bytes `tokenize` does not treat as boundaries — a
//! vertical tab, a form feed, a non-breaking space — and `tokenize` glues them
//! to what follows, including to a digit that is really a redirect's fd
//! number: `\x0b2>&1` is a word and a `>&1` redirect to the lexer, but the
//! command `2>&1` once trimmed. The rewrite must read the trimmed text as a
//! fresh lex of it would, and the difference only shows through a rewrite
//! that depends on what the trailing redirect is — a trusted project filter
//! that matches every command, as here.
//!
//! Unix only: the binary resolves its config directory through `$HOME`, and
//! only Unix takes that from the environment.
#![cfg(unix)]

use std::path::Path;
use std::process::Command;

/// A project filter matching every command, so that one which is rewritten at
/// all is rewritten, and only the trailing-redirect handling decides where.
const CATCH_ALL: &str = "schema_version = 1\n\n[filters.anything]\ndescription = \"probe\"\nmatch_command = \"^.\"\nmax_lines = 5\n";

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(dir.path().join("work").join(".rtk")).expect("project dir");
    std::fs::write(
        dir.path().join("work").join(".rtk").join("filters.toml"),
        CATCH_ALL,
    )
    .expect("filters");
    dir
}

/// `rtk rewrite <cmd>` as one argv, run in the project, returning stdout.
fn rewrite(dir: &Path, cmd: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_rtk"))
        .args(["rewrite", cmd])
        .current_dir(dir.join("work"))
        .env("HOME", dir)
        .env("XDG_CONFIG_HOME", dir.join(".config"))
        .env("XDG_DATA_HOME", dir.join(".local").join("share"))
        .env("RTK_DB_PATH", dir.join("rtk.db"))
        .env("RTK_TELEMETRY_DISABLED", "1")
        .env("RTK_TRUST_PROJECT_FILTERS", "1")
        .env("CI", "1")
        .output()
        .expect("run rtk");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn a_trim_only_byte_glued_to_an_fd_number_reads_as_the_redirect_it_becomes() {
    let dir = project();
    for (cmd, expected) in [
        // Left alone: once trimmed the whole command is a redirect, which no
        // filter is asked about.
        ("noglob \x0b2>&1", ""),
        ("FOO=1 \x0b12>&-", ""),
        ("nice -n 5 \x0b2>&1", ""),
        ("exec \x0c2>&1", ""),
        // Rewritten around the redirect, which stays where it was.
        ("uv run \u{a0}2>&1", "rtk uv run \u{a0}2>&1"),
        ("git status; \x0b2>&1", "rtk git status; \x0b2>&1"),
        ("a &&  2<&0", "rtk a &&  2<&0"),
        ("ls\n\u{a0}2>/dev/null", "rtk ls\n\u{a0}2>/dev/null"),
        ("git status &&\r\n\x0b2>&1", "rtk git status &&\r\n\x0b2>&1"),
        ("( git status )\x0b2>&1", "( rtk git status )\x0b2>&1"),
    ] {
        assert_eq!(rewrite(dir.path(), cmd), expected, "{cmd:?}");
    }
}
