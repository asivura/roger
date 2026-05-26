//! `tmux has-session -t <name>`
//!
//! TmuxBackend uses this to check whether a session named `<name>`
//! exists before deciding whether to call `new-session`. Since we
//! manage exactly one Zellij session (whichever the shim runs inside),
//! we always answer "yes" — exit 0 — so TmuxBackend takes the
//! attach-existing path. The `-t <name>` argument is ignored.

use std::process::ExitCode;

pub fn run(_args: &[String]) -> ExitCode {
    ExitCode::SUCCESS
}
