//! `tmux display-message -p '#{pane_id}'`
//!
//! TmuxBackend uses this at startup to learn the current pane id. We
//! return the value of `ZELLIJ_PANE_ID` (the env var Zellij sets in
//! every pane), rendered as `%<n>`. If that env var is missing —
//! shouldn't happen inside a Zellij session, but defensive — we fall
//! back to the first tracked teammate from `team.list`, and finally
//! to `%0` if even that is empty.

use std::env;
use std::process::ExitCode;

use roger_proto::TeamListResult;
use serde_json::json;

use crate::commands::report_rpc_error;
use crate::pane_id::PaneId;
use crate::rpc;

pub fn run(_args: &[String]) -> ExitCode {
    // We ignore the format string in `-p '<format>'` — TmuxBackend
    // only ever uses `#{pane_id}` here, and that's exactly what we
    // print.

    if let Some(zellij_pane_id) = env::var("ZELLIJ_PANE_ID").ok().and_then(|s| parse_id(&s)) {
        println!("{}", PaneId(zellij_pane_id));
        return ExitCode::SUCCESS;
    }

    // Fallback: ask the plugin which panes it knows about.
    let result: Result<TeamListResult, _> = rpc::call("team.list", json!({}));
    match result {
        Ok(list) => {
            let id = list.panes.first().map(|p| p.pane_id).unwrap_or(0);
            println!("{}", PaneId(id));
            ExitCode::SUCCESS
        }
        Err(e) => report_rpc_error("team.list", e),
    }
}

fn parse_id(s: &str) -> Option<u32> {
    s.parse().ok()
}
