//! tmux subcommand handlers.
//!
//! [`dispatch`] is the central entrypoint called from `main`. Each
//! tmux verb routes to a per-handler module below. The dispatcher
//! also enforces a catch-all: unknown subcommands log a warning and
//! exit 0, so future Claude additions degrade rather than crash.

use std::process::ExitCode;

mod display_message;
mod has_session;
mod kill_pane;
mod list_panes;
mod send_keys;
mod spawn;
mod stubs;

/// Route `subcommand` + `args` to the matching handler.
pub fn dispatch(subcommand: &str, args: &[String]) -> ExitCode {
    match subcommand {
        // The eight real ops (#9).
        "display-message" => display_message::run(args),
        "has-session" => has_session::run(args),
        "new-session" | "new-window" | "split-window" => spawn::run(subcommand, args),
        "list-panes" => list_panes::run(args),
        "send-keys" => send_keys::run(args),
        "kill-pane" => kill_pane::run(args),

        // Cosmetic ops (#10) — accepted and ignored.
        "select-pane" | "set-option" | "select-layout" | "resize-pane" | "break-pane"
        | "join-pane" | "set-window-option" | "set-environment" => stubs::accept_silent(),

        // Catch-all (#10) — anything we don't recognize logs a warning
        // and exits 0. Future Claude additions degrade rather than
        // crash; an operator inspecting plugin logs sees what was
        // attempted.
        other => stubs::accept_with_warning(other),
    }
}

/// Convert an `rpc::RpcError` into stderr output + ExitCode. Shared
/// shape across all RPC-using handlers.
pub(crate) fn report_rpc_error(method: &str, err: crate::rpc::RpcError) -> ExitCode {
    eprintln!("roger-shim: {} failed: {}", method, err);
    ExitCode::FAILURE
}
