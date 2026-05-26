//! roger-shim — Claude Code TmuxBackend → roger plugin RPC translator.
//!
//! When this binary lives earlier in `PATH` than the real `tmux`, every
//! `tmux <subcommand>` invocation from Claude Code's TmuxBackend lands
//! here. We translate the subcommand into an RPC call against the roger
//! plugin via `zellij pipe` and print the tmux-shaped response that
//! TmuxBackend expects.
//!
//! v0 stub: no translation yet. Parse argv enough to name the
//! subcommand in a stderr message, then exit 0 so callers see a benign
//! no-op rather than a hard error.
//!
//! Real translation lands in:
//!   #9  — the six real commands (split-window, send-keys, kill-pane,
//!         list-panes, has-session, display-message, new-session,
//!         new-window)
//!   #10 — cosmetic no-ops (select-pane, set-option, select-layout,
//!         resize-pane, break-pane, join-pane) + TMUX env detection.

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let subcommand = args.get(1).map(String::as_str).unwrap_or("(no subcommand)");
    eprintln!(
        "roger-shim: tmux {} — not yet implemented (see issues #9, #10). \
         Exiting 0 so the caller sees a benign no-op.",
        subcommand
    );
    ExitCode::SUCCESS
}
