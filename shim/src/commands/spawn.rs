//! `tmux new-session` / `new-window` / `split-window`
//!
//! All three create an empty pane in real tmux; TmuxBackend then drives
//! `send-keys` separately to launch the teammate process. We can't
//! create an empty pane through `team.spawn` (the plugin's
//! `open_command_pane` needs a real command), so we spawn the user's
//! `$SHELL` as a placeholder process. The follow-up `send-keys` then
//! types `claude --agent-id ...` into that shell, which exec's the
//! teammate. This matches real tmux's pane-then-command flow with a
//! brief shell-prompt flash that Claude doesn't read.
//!
//! See `docs/shim.md`'s "v0.1 limitations" section for why this is
//! the current approach and what option 2 / option 3 would look like
//! in a future iteration.

use std::env;
use std::process::ExitCode;

use roger_proto::SpawnResult;
use serde_json::json;

use crate::commands::report_rpc_error;
use crate::pane_id::PaneId;
use crate::rpc;

pub fn run(_subcommand: &str, args: &[String]) -> ExitCode {
    let parsed = parse_spawn_args(args);

    let shell = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
    let cwd = env::current_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "/".to_string());

    // The plugin needs `argv` non-empty; the shell is our placeholder.
    let argv: Vec<String> = vec![shell];

    let agent_id = parsed
        .name
        .clone()
        .or(parsed.session.clone())
        .unwrap_or_else(|| "teammate".to_string());
    let display_name = parsed.name.unwrap_or_else(|| "teammate".to_string());

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
