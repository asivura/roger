//! `tmux kill-pane -t <pane>`
//!
//! Maps to `team.kill` on the plugin. If the pane is already gone (the
//! plugin returns `INVALID_PARAMS "unknown pane_id"`), we still exit
//! 0 — kill-of-already-dead is the desired end state, not a failure
//! the caller should see.

use std::process::ExitCode;

use roger_proto::OkResult;
use serde_json::json;

use crate::commands::report_rpc_error;
use crate::pane_id;
use crate::rpc;

pub fn run(args: &[String]) -> ExitCode {
    let Some(pane) = parse_target(args) else {
        eprintln!(
            "roger-shim: kill-pane: expected `-t <pane>`, got: {:?}",
            args
        );
        return ExitCode::FAILURE;
    };

    let result: Result<OkResult, _> = rpc::call("team.kill", json!({ "pane_id": pane.0 }));
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) if rpc::is_unknown_pane_id(&e) => {
            // Idempotent: already gone is fine.
            ExitCode::SUCCESS
        }
        Err(e) => report_rpc_error("team.kill", e),
    }
}

fn parse_target(args: &[String]) -> Option<pane_id::PaneId> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "-t" {
            let raw = iter.next()?;
            return pane_id::parse(raw).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_target() {
        let args = vec_str(&["-t", "%42"]);
        assert_eq!(parse_target(&args).map(|p| p.0), Some(42));
    }

    #[test]
    fn parses_target_with_other_flags() {
        let args = vec_str(&["-a", "-t", "%99"]);
        assert_eq!(parse_target(&args).map(|p| p.0), Some(99));
    }

    #[test]
    fn rejects_missing_target() {
        let args = vec_str(&["-a"]);
        assert!(parse_target(&args).is_none());
    }

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
}
