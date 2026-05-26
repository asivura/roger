//! `tmux list-panes -t <window> -F '#{pane_id}'`
//!
//! TmuxBackend uses this to enumerate currently-tracked teammate panes.
//! We call `team.list` on the plugin and print one `%<id>` per line.
//! An empty list is fine — that's exit 0 with no stdout, matching
//! tmux's behavior when a session has no panes (impossible in real
//! tmux, but the format is identical).

use std::process::ExitCode;

use roger_proto::TeamListResult;
use serde_json::json;

use crate::commands::report_rpc_error;
use crate::pane_id::PaneId;
use crate::rpc;

pub fn run(_args: &[String]) -> ExitCode {
    let result: Result<TeamListResult, _> = rpc::call("team.list", json!({}));
    match result {
        Ok(list) => {
            for pane in &list.panes {
                println!("{}", PaneId(pane.pane_id));
            }
            ExitCode::SUCCESS
        }
        Err(e) => report_rpc_error("team.list", e),
    }
}
