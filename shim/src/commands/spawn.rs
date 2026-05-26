//! `tmux new-session` / `new-window` / `split-window`
//!
//! All three create an empty pane in real tmux; TmuxBackend then drives
//! `send-keys` separately to launch the teammate process. Because
//! `team.spawn` requires non-empty argv, we spawn the
//! [`SPAWN_PLACEHOLDER_SNIPPET`] shell snippet — see its docstring
//! for the WHY and the trust-contract caveat.
//!
//! See `docs/shim.md` for the operator-facing description of this
//! flow and the resulting auto-close-on-clean-exit behavior.

use std::env;
use std::process::ExitCode;

use roger_proto::SpawnResult;
use serde_json::json;

use crate::commands::report_rpc_error;
use crate::pane_id::PaneId;
use crate::rpc;

/// Shell snippet spawned as the pane's main process. It:
///
/// 1. reads one line from stdin (which the follow-up `send-keys`
///    delivers — typically `claude --agent-id …`),
/// 2. `eval`s `exec $cmd`, replacing this shell with the command.
///
/// After step 2 the pane's main process *is* the teammate, so when
/// the teammate exits Zellij emits `CommandPaneExited` and the
/// plugin's lifecycle handler decides whether to auto-close the
/// pane.
///
/// **Trust contract:** `eval` interprets `$cmd` with full shell
/// semantics, so hostile shell metacharacters in the `send-keys`
/// payload would escape. Acceptable under the v0.1 single-producer
/// model (Claude Code's TmuxBackend via roger-shim is the only
/// expected source). See `docs/trust-model.md`.
const SPAWN_PLACEHOLDER_SNIPPET: &str = r#"IFS= read -r cmd && eval "exec $cmd""#;

pub fn run(_subcommand: &str, args: &[String]) -> ExitCode {
    let parsed = parse_spawn_args(args);

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let cwd = env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "/".to_string());

    let argv: Vec<String> = vec![
        shell,
        "-c".to_string(),
        SPAWN_PLACEHOLDER_SNIPPET.to_string(),
    ];

    // agent_id derivation: prefer `<window>@<session>` when both are
    // present (matches the canonical format the RPC docs use). Fall
    // back to whichever single piece is available, finally to
    // `"teammate"`. Correctness reviewer (PR #58) flagged that the
    // prior version dropped the session entirely.
    let agent_id = match (parsed.name.as_deref(), parsed.session.as_deref()) {
        (Some(name), Some(session)) => format!("{}@{}", name, session),
        (Some(name), None) => name.to_string(),
        (None, Some(session)) => session.to_string(),
        (None, None) => "teammate".to_string(),
    };
    let display_name = parsed
        .name
        .as_deref()
        .or(parsed.session.as_deref())
        .unwrap_or("teammate")
        .to_string();

    let result: Result<SpawnResult, _> = rpc::call(
        "team.spawn",
        json!({
            "agent_id": agent_id,
            "name": display_name,
            "cwd": cwd,
            "argv": argv,
        }),
    );
    match result {
        Ok(SpawnResult { pane_id }) => {
            println!("{}", PaneId(pane_id));
            ExitCode::SUCCESS
        }
        Err(e) => report_rpc_error("team.spawn", e),
    }
}

/// Subset of new-session / new-window / split-window argv we actually
/// look at. Everything else (`-d`, `-P`, `-F`, `-h`, `-v`, `-l <pct>`)
/// is accepted and ignored — Zellij decides layout.
#[derive(Debug, Default, PartialEq, Eq)]
struct SpawnArgs {
    session: Option<String>,
    name: Option<String>,
}

fn parse_spawn_args(args: &[String]) -> SpawnArgs {
    let mut out = SpawnArgs::default();
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-s" => {
                out.session = iter.next().cloned();
            }
            "-n" => {
                out.name = iter.next().cloned();
            }
            // Flags we explicitly accept and skip their argument when
            // they take one. The order matters: `-l` and `-t` take an
            // argument, `-d` / `-P` / `-h` / `-v` don't.
            "-t" | "-l" | "-F" | "-c" | "-x" | "-y" | "-e" => {
                iter.next();
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_new_session_typical() {
        let args = vec_str(&[
            "-d",
            "-s",
            "myteam",
            "-n",
            "researcher",
            "-P",
            "-F",
            "#{pane_id}",
        ]);
        assert_eq!(
            parse_spawn_args(&args),
            SpawnArgs {
                session: Some("myteam".into()),
                name: Some("researcher".into()),
            }
        );
    }

    #[test]
    fn parses_split_window_with_layout_flags() {
        let args = vec_str(&[
            "-t",
            "%17",
            "-h",
            "-l",
            "50%",
            "-n",
            "linter",
            "-P",
            "-F",
            "#{pane_id}",
        ]);
        assert_eq!(
            parse_spawn_args(&args),
            SpawnArgs {
                session: None,
                name: Some("linter".into()),
            }
        );
    }

    #[test]
    fn handles_empty_argv() {
        let args: Vec<String> = vec![];
        assert_eq!(parse_spawn_args(&args), SpawnArgs::default());
    }

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
}
