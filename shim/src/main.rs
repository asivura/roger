//! roger-shim — Claude Code TmuxBackend → roger plugin RPC translator.
//!
//! When this binary lives earlier in `PATH` than the real `tmux`, every
//! `tmux <subcommand>` invocation from Claude Code's TmuxBackend lands
//! here. We translate the subcommand into a JSON-RPC call against the
//! roger plugin via `zellij pipe` and print the tmux-shaped output
//! that TmuxBackend expects.
//!
//! See `docs/shim.md` for the full command surface and
//! `docs/rpc-protocol.md` for the wire protocol.

use std::env;
use std::process::ExitCode;

mod commands;
mod pane_id;
mod rpc;

/// Exit code returned when the shim refuses to run because the
/// environment isn't a Zellij session. Distinct from the catch-all
/// failure code (1) so a caller can distinguish "you ran this in the
/// wrong place" from "the operation failed".
const EXIT_NOT_IN_ZELLIJ: u8 = 2;

fn main() -> ExitCode {
    // TMUX env detection. The shim only makes sense inside a Zellij
    // session — outside one, every RPC call would fail. Surface a
    // clear error rather than a cryptic "zellij pipe" failure.
    if env::var_os("ZELLIJ_SESSION_NAME").is_none() {
        eprintln!(
            "roger-shim: not running inside Zellij (ZELLIJ_SESSION_NAME unset). \
             This binary is the `tmux`-compatible shim for Claude Code; \
             it cannot run outside a Zellij session."
        );
        return ExitCode::from(EXIT_NOT_IN_ZELLIJ);
    }

    // Strip the global `-S <socket>` / `-L <name>` prefixes tmux
    // accepts before any subcommand. Claude's TmuxBackend occasionally
    // emits these; we accept and ignore. Done by hand because
    // clap's `external_subcommand` mode is incompatible with other
    // top-level args.
    let argv: Vec<String> = env::args().skip(1).collect();
    let argv = strip_global_flags(argv);

    let Some((subcommand, args)) = argv.split_first() else {
        eprintln!("roger-shim: no subcommand given");
        return ExitCode::FAILURE;
    };
    commands::dispatch(subcommand, args)
}

/// Drop leading `-S <path>` / `-L <name>` pairs from `argv`. Leaves
/// anything else (including unknown leading flags) untouched.
fn strip_global_flags(argv: Vec<String>) -> Vec<String> {
    let mut iter = argv.into_iter();
    let mut out: Vec<String> = Vec::new();
    let mut still_stripping = true;
    while let Some(arg) = iter.next() {
        if still_stripping {
            match arg.as_str() {
                "-S" | "-L" => {
                    // Drop the flag's argument too.
                    let _ = iter.next();
                    continue;
                }
                _ => still_stripping = false,
            }
        }
        out.push(arg);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_global_flags;

    #[test]
    fn strips_dash_s_pair() {
        let in_ = vec_str(&["-S", "/tmp/sock", "display-message", "-p", "#{pane_id}"]);
        assert_eq!(
            strip_global_flags(in_),
            vec_str(&["display-message", "-p", "#{pane_id}"])
        );
    }

    #[test]
    fn strips_dash_l_pair() {
        let in_ = vec_str(&["-L", "default", "has-session", "-t", "foo"]);
        assert_eq!(
            strip_global_flags(in_),
            vec_str(&["has-session", "-t", "foo"])
        );
    }

    #[test]
    fn strips_both() {
        let in_ = vec_str(&["-L", "default", "-S", "/tmp/sock", "list-panes"]);
        assert_eq!(strip_global_flags(in_), vec_str(&["list-panes"]));
    }

    #[test]
    fn leaves_argv_without_globals_unchanged() {
        let in_ = vec_str(&["new-session", "-d", "-s", "x"]);
        assert_eq!(strip_global_flags(in_.clone()), in_);
    }

    #[test]
    fn doesnt_strip_after_subcommand() {
        // -L appearing AFTER the subcommand isn't a global prefix; it
        // belongs to the subcommand. Leave it alone.
        let in_ = vec_str(&["send-keys", "-L", "C-c"]);
        assert_eq!(strip_global_flags(in_.clone()), in_);
    }

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
}
