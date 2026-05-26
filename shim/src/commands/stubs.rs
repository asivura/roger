//! Cosmetic / catch-all handlers (#10).
//!
//! Claude Code's TmuxBackend issues a number of cosmetic tmux commands
//! (pane styling, layout, titles, etc.) that don't have a meaningful
//! analogue in Zellij — Zellij does its own layout. We accept the
//! invocation, do nothing, exit 0. This lets TmuxBackend's
//! best-effort chrome calls succeed silently rather than fail and
//! halt the wider workflow.

use std::process::ExitCode;

/// For commands we *know* we want to ignore (whitelisted list in the
/// dispatcher). Silent — no stderr noise on every pane-styling call.
pub fn accept_silent() -> ExitCode {
    ExitCode::SUCCESS
}

/// For commands we don't recognize at all. Logs a one-line warning so
/// an operator inspecting plugin logs sees what was attempted, but
/// still exits 0 so Claude additions degrade rather than crash the
/// shim.
pub fn accept_with_warning(subcommand: &str) -> ExitCode {
    eprintln!(
        "roger-shim: unknown tmux subcommand {:?}; treating as no-op (Phase D will revisit)",
        subcommand
    );
    ExitCode::SUCCESS
}
