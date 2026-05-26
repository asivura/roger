//! `tmux send-keys -t <pane> '<text>' [Enter]`
//!
//! TmuxBackend uses this to drive keystrokes into a pane's PTY. The
//! literal token `Enter` in argv (or `C-m`) translates to a `\r`
//! appended to the text — that's how tmux signals "submit this line".
//! Everything else is passed through verbatim.
//!
//! Maps to `team.send` on the plugin. Synchronous on the RPC; see
//! `docs/rpc-protocol.md`'s "Result semantics" note on what
//! `{ok: true}` means in practice (dispatched, not necessarily
//! delivered).

use std::process::ExitCode;

use roger_proto::OkResult;
use serde_json::json;

use crate::commands::report_rpc_error;
use crate::pane_id;
use crate::rpc;

pub fn run(args: &[String]) -> ExitCode {
    let Some((pane, keys)) = parse_args(args) else {
        eprintln!(
            "roger-shim: send-keys: expected `-t <pane> <text>... [Enter|C-m]`, got: {:?}",
            args
        );
        return ExitCode::FAILURE;
    };

    let text = render_keys(&keys);

    let result: Result<OkResult, _> = rpc::call(
        "team.send",
        json!({
            "pane_id": pane.0,
            "text": text,
        }),
    );
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => report_rpc_error("team.send", e),
    }
}

/// Extract `(target_pane, key_tokens)` from `args`. Returns `None` if
/// the shape doesn't match the expected `-t <pane> <key>...` form.
fn parse_args(args: &[String]) -> Option<(pane_id::PaneId, Vec<String>)> {
    let mut iter = args.iter();
    let mut target: Option<pane_id::PaneId> = None;
    let mut rest: Vec<String> = Vec::new();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-t" => {
                let raw = iter.next()?;
                target = pane_id::parse(raw).ok();
                target?;
            }
            // Ignore flags TmuxBackend sometimes passes but which we
            // don't act on at the per-keystroke level (literal-mode,
            // no-newline). The `Enter` / `C-m` handling lives in
            // `render_keys`.
            "-l" | "-X" | "-x" => {}
            _ => rest.push(arg.clone()),
        }
    }
    let target = target?;
    Some((target, rest))
}

/// Translate the tmux key-token list into a literal string suitable
/// for `team.send`. The token `Enter` (or `C-m`) becomes `\r`; every
/// other token is concatenated verbatim. Tmux concatenates multi-arg
/// text inputs space-free, NOT space-joined — `send-keys "foo" "bar"`
/// sends `foobar`. We match that.
fn render_keys(tokens: &[String]) -> String {
    let mut out = String::new();
    for tok in tokens {
        match tok.as_str() {
            "Enter" | "C-m" => out.push('\r'),
            other => out.push_str(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pane_and_keys() {
        let args = vec_str(&["-t", "%17", "hello world", "Enter"]);
        let (pane, keys) = parse_args(&args).expect("parses");
        assert_eq!(pane.0, 17);
        assert_eq!(keys, vec_str(&["hello world", "Enter"]));
    }

    #[test]
    fn rejects_missing_pane() {
        let args = vec_str(&["hello", "Enter"]);
        assert!(parse_args(&args).is_none());
    }

    #[test]
    fn rejects_bad_pane_form() {
        let args = vec_str(&["-t", "17", "hello"]);
        assert!(parse_args(&args).is_none());
    }

    #[test]
    fn render_handles_enter() {
        let keys = vec_str(&["claude --agent-id foo", "Enter"]);
        assert_eq!(render_keys(&keys), "claude --agent-id foo\r");
    }

    #[test]
    fn render_handles_cm() {
        let keys = vec_str(&["echo hi", "C-m"]);
        assert_eq!(render_keys(&keys), "echo hi\r");
    }

    #[test]
    fn render_concatenates_multiple_text_tokens_with_no_space() {
        // Matches tmux: send-keys "foo" "bar" → sends "foobar"
        let keys = vec_str(&["foo", "bar", "Enter"]);
        assert_eq!(render_keys(&keys), "foobar\r");
    }

    #[test]
    fn render_passes_through_other_tokens_verbatim() {
        let keys = vec_str(&["a", " ", "b"]);
        assert_eq!(render_keys(&keys), "a b");
    }

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
}
