//! Wire types for the roger RPC protocol between the `roger-shim` CLI
//! and the `roger` Wasm plugin, transported over Zellij CLI pipes.
//!
//! This crate has no `zellij-tile` dependency, by design. Putting the
//! wire types here means we can:
//!   - build and run unit tests on the host target (zellij-tile won't
//!     link on host, the plugin crate can't be host-tested);
//!   - share these types between the plugin and the shim verbatim,
//!     so the wire shape can't drift between sender and receiver.
//!
//! Full protocol reference: `docs/rpc-protocol.md` in the repo root.
//!
//! Method surface (`team.list` ships in #5; #6-#7 land the others):
//!
//! - `team.list`   → list panes currently tracked by `roger`
//! - `team.spawn`  → spawn a teammate as a new Zellij pane (#6)
//! - `team.send`   → write text into a teammate pane (#7)
//! - `team.kill`   → close a teammate pane (#7)

use serde::{Deserialize, Serialize};

/// JSON-RPC-style request, deserialized from a `zellij pipe` payload.
///
/// `params` is left as raw JSON so each handler can deserialize into
/// its own typed params struct without inflating this top-level type.
/// `team.list` takes no params and never reads the field; `team.spawn`
/// (#6) and `team.send` / `team.kill` (#7) will.
#[derive(Debug, Deserialize)]
pub struct Request {
    pub method: String,
    pub id: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Response envelope. Exactly one of `result` / `error` is set; the
/// other is omitted from the serialized form via `skip_serializing_if`.
///
/// The "exactly one" invariant is enforced by convention via
/// `Response::ok` / `Response::err`; a stronger compile-time guarantee
/// (untagged enum) is tracked in #37.
#[derive(Debug, Serialize)]
pub struct Response {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorPayload>,
}

impl Response {
    /// Success response. Sets `result`; leaves `error` `None`.
    pub fn ok(id: &str, result: serde_json::Value) -> Self {
        Self {
            id: id.to_string(),
            result: Some(result),
            error: None,
        }
    }

    /// Error response. Sets `error`; leaves `result` `None`.
    pub fn err(id: &str, code: i32, message: impl Into<String>) -> Self {
        Self {
            id: id.to_string(),
            result: None,
            error: Some(ErrorPayload {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub code: i32,
    pub message: String,
}

/// Per-teammate state the plugin tracks. Returned by `team.list`.
///
/// `exited` flips to `true` when Zellij emits `CommandPaneExited` for
/// the underlying pane; the pane (and its scrollback) survives until
/// the operator closes it or re-runs it. `exit_code` captures the
/// process's exit status when known — `None` while the process is
/// still running or when Zellij reports no exit code, `Some(code)`
/// after `CommandPaneExited`. Omitted from the wire format when
/// `None` so existing `team.list` consumers are unaffected.
///
/// **Invariant:** `exit_code` is `Some(_)` only when `exited == true`
/// — the handler flips both fields together (`on_command_pane_exited`
/// sets both; `on_command_pane_rerun` clears both). The wire format
/// permits the desynced shape, but consumers can rely on the
/// invariant for any teammate the plugin emits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TeammatePaneInfo {
    pub agent_id: String,
    pub pane_id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub command: Option<String>,
    pub exited: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub exit_code: Option<i32>,
}

/// Result type for the `team.list` method.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct TeamListResult {
    pub panes: Vec<TeammatePaneInfo>,
}

/// Params for the `team.spawn` method.
///
/// The shim builds this from Claude Code's `TmuxBackend` invocation:
/// `argv` is the command + args that will start the teammate process
/// (`["claude", "--agent-id", "researcher@my-team", ...]`); `cwd` is
/// the working directory; `name` is the human-readable label that
/// surfaces in the Zellij pane title; `color` (optional) hints the
/// border style. `agent_id` is the unique identifier the shim uses
/// to address the teammate from Claude Code's bookkeeping — the
/// plugin echoes it back in `team.list`.
#[derive(Debug, Clone, Deserialize)]
pub struct SpawnParams {
    pub agent_id: String,
    pub name: String,
    pub cwd: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub color: Option<String>,
}

/// Result type for the `team.spawn` method. The `pane_id` is the
/// Zellij pane the plugin opened for the teammate, returned so the
/// shim can address subsequent `team.send` / `team.kill` calls to it.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SpawnResult {
    pub pane_id: u32,
}

/// Params for the `team.send` method. Writes `text` into the
/// teammate's PTY (the inner half of TmuxBackend's `send-keys`
/// semantics).
///
/// **Trust contract:** `text` is delivered to the PTY as-is — ANSI
/// CSI/OSC sequences, control characters, and `\r` (which causes a
/// shell to execute the buffered line) are all passed through. This
/// is the same trust model as
/// [`tmux send-keys`](https://man.openbsd.org/tmux.1#send-keys) and
/// is fine while the sole producer is `roger-shim` relaying from
/// Claude Code's own TmuxBackend. **Do not route untrusted output
/// (e.g. another teammate's stdout) through this method without
/// adding a sanitization layer first** — the same code path then
/// becomes a terminal-injection sink (OSC 52 clipboard writes,
/// OSC 8 hyperlinks, title spoof, cursor-position queries).
#[derive(Debug, Clone, Deserialize)]
pub struct SendParams {
    pub pane_id: u32,
    pub text: String,
}

/// Params for the `team.kill` method. Closes the teammate's pane
/// and removes it from `State::teammates`.
#[derive(Debug, Clone, Deserialize)]
pub struct KillParams {
    pub pane_id: u32,
}

/// Result type for `team.send` and `team.kill`. Both are
/// fire-and-forget on the Zellij side; success means the plugin
/// accepted the request and dispatched the underlying call.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OkResult {
    pub ok: bool,
}

/// JSON-RPC-style error codes. The numeric values mirror the
/// [JSON-RPC 2.0 reserved range](https://www.jsonrpc.org/specification#error_object)
/// so any future generic JSON-RPC client knows how to interpret them
/// without a roger-specific dispatch table.
///
/// `INVALID_REQUEST` and `INVALID_PARAMS` are defined now even though
/// only `PARSE_ERROR`, `METHOD_NOT_FOUND`, `INVALID_REQUEST`, and
/// `INTERNAL_ERROR` are referenced in the plugin today — `INVALID_PARAMS`
/// will be used by `team.spawn` (#6) and `team.send` / `team.kill` (#7).
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Roger-specific: server-error range per JSON-RPC 2.0. Returned
    /// when `team.spawn` fails to materialize a Zellij pane (e.g.
    /// argv is empty so there's no command to run).
    pub const SPAWN_FAILED: i32 = -32001;
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Request deserialization ----------------------------------

    #[test]
    fn request_parses_minimal_team_list() {
        let r: Request = serde_json::from_str(r#"{"method":"team.list","id":"abc"}"#).unwrap();
        assert_eq!(r.method, "team.list");
        assert_eq!(r.id, "abc");
        assert!(r.params.is_null(), "missing params should default to Null");
    }

    #[test]
    fn request_parses_with_empty_params_object() {
        let r: Request =
            serde_json::from_str(r#"{"method":"team.list","id":"abc","params":{}}"#).unwrap();
        assert!(r.params.is_object());
    }

    #[test]
    fn request_rejects_missing_method() {
        let r: Result<Request, _> = serde_json::from_str(r#"{"id":"abc"}"#);
        assert!(r.is_err(), "id-only payload should not deserialize");
    }

    #[test]
    fn request_rejects_missing_id() {
        let r: Result<Request, _> = serde_json::from_str(r#"{"method":"team.list"}"#);
        assert!(r.is_err(), "method-only payload should not deserialize");
    }

    // --- Response serialization (the protocol's exactly-one invariant)

    #[test]
    fn response_ok_serializes_with_result_only() {
        let r = Response::ok("abc", serde_json::json!({"panes": []}));
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""result":{"panes":[]}"#), "got: {}", s);
        assert!(
            !s.contains("\"error\""),
            "error field must be omitted, got: {}",
            s
        );
    }

    #[test]
    fn response_err_serializes_with_error_only() {
        let r = Response::err("abc", error_codes::METHOD_NOT_FOUND, "not found");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""code":-32601"#), "got: {}", s);
        assert!(s.contains(r#""message":"not found""#), "got: {}", s);
        assert!(
            !s.contains("\"result\""),
            "result field must be omitted, got: {}",
            s
        );
    }

    // --- Error codes match JSON-RPC 2.0 reserved range -----------

    #[test]
    fn error_codes_match_jsonrpc_2_0() {
        assert_eq!(error_codes::PARSE_ERROR, -32700);
        assert_eq!(error_codes::INVALID_REQUEST, -32600);
        assert_eq!(error_codes::METHOD_NOT_FOUND, -32601);
        assert_eq!(error_codes::INVALID_PARAMS, -32602);
        assert_eq!(error_codes::INTERNAL_ERROR, -32603);
    }

    // --- Method-specific result types -----------------------------

    #[test]
    fn team_list_result_serializes_panes_array() {
        let r = TeamListResult {
            panes: vec![TeammatePaneInfo {
                agent_id: "researcher@my-team".into(),
                pane_id: 17,
                name: "researcher".into(),
                command: Some("claude --agent-id ...".into()),
                exited: false,
                exit_code: None,
            }],
        };
        let s = serde_json::to_string(&r).unwrap();
        assert!(
            s.contains(r#""agent_id":"researcher@my-team""#),
            "got: {}",
            s
        );
        assert!(s.contains(r#""pane_id":17"#), "got: {}", s);
        assert!(s.contains(r#""exited":false"#), "got: {}", s);
    }

    #[test]
    fn team_list_result_serializes_empty_panes_array() {
        let r = TeamListResult { panes: vec![] };
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"panes":[]}"#);
    }

    // --- team.spawn types ----------------------------------------

    #[test]
    fn spawn_params_parses_happy_path() {
        let p: SpawnParams = serde_json::from_str(
            r#"{"agent_id":"researcher@my-team","name":"researcher","cwd":"/tmp",
                "argv":["claude","--agent-id","researcher@my-team"],"color":"blue"}"#,
        )
        .unwrap();
        assert_eq!(p.agent_id, "researcher@my-team");
        assert_eq!(p.name, "researcher");
        assert_eq!(p.cwd, "/tmp");
        assert_eq!(p.argv, vec!["claude", "--agent-id", "researcher@my-team"]);
        assert_eq!(p.color.as_deref(), Some("blue"));
    }

    #[test]
    fn spawn_params_defaults_color_to_none() {
        let p: SpawnParams =
            serde_json::from_str(r#"{"agent_id":"a","name":"n","cwd":"/tmp","argv":["x"]}"#)
                .unwrap();
        assert!(p.color.is_none());
    }

    #[test]
    fn spawn_params_rejects_missing_required_fields() {
        // Missing agent_id
        let r: Result<SpawnParams, _> =
            serde_json::from_str(r#"{"name":"n","cwd":"/tmp","argv":["x"]}"#);
        assert!(r.is_err(), "missing agent_id should be rejected");
        // Missing argv
        let r: Result<SpawnParams, _> =
            serde_json::from_str(r#"{"agent_id":"a","name":"n","cwd":"/tmp"}"#);
        assert!(r.is_err(), "missing argv should be rejected");
    }

    #[test]
    fn spawn_result_serializes_pane_id() {
        let r = SpawnResult { pane_id: 17 };
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"pane_id":17}"#);
    }

    #[test]
    fn spawn_failed_error_code() {
        assert_eq!(error_codes::SPAWN_FAILED, -32001);
    }

    #[test]
    fn response_err_with_invalid_params_serializes_correctly() {
        // Exercises the INVALID_PARAMS code through Response::err,
        // which previously had no test coverage.
        let r = Response::err("abc", error_codes::INVALID_PARAMS, "missing argv");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""code":-32602"#), "got: {}", s);
        assert!(s.contains(r#""message":"missing argv""#), "got: {}", s);
    }

    #[test]
    fn response_err_with_spawn_failed_serializes_correctly() {
        let r = Response::err("abc", error_codes::SPAWN_FAILED, "empty argv");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""code":-32001"#), "got: {}", s);
    }

    // --- team.send / team.kill ----------------------------------

    #[test]
    fn send_params_parses_happy_path() {
        let p: SendParams = serde_json::from_str(r#"{"pane_id":17,"text":"hello\n"}"#).unwrap();
        assert_eq!(p.pane_id, 17);
        assert_eq!(p.text, "hello\n");
    }

    #[test]
    fn send_params_rejects_missing_fields() {
        let r: Result<SendParams, _> = serde_json::from_str(r#"{"pane_id":17}"#);
        assert!(r.is_err(), "missing text should be rejected");
        let r: Result<SendParams, _> = serde_json::from_str(r#"{"text":"x"}"#);
        assert!(r.is_err(), "missing pane_id should be rejected");
    }

    #[test]
    fn kill_params_parses_happy_path() {
        let p: KillParams = serde_json::from_str(r#"{"pane_id":17}"#).unwrap();
        assert_eq!(p.pane_id, 17);
    }

    #[test]
    fn kill_params_rejects_missing_pane_id() {
        let r: Result<KillParams, _> = serde_json::from_str(r#"{}"#);
        assert!(r.is_err(), "missing pane_id should be rejected");
    }

    #[test]
    fn ok_result_serializes_as_object_with_ok_field() {
        let r = OkResult { ok: true };
        let s = serde_json::to_string(&r).unwrap();
        assert_eq!(s, r#"{"ok":true}"#);
    }

    // --- existing tests below -----------------------------------

    #[test]
    fn teammate_pane_info_omits_command_when_none() {
        let t = TeammatePaneInfo {
            agent_id: "a".into(),
            pane_id: 1,
            name: "n".into(),
            command: None,
            exited: false,
            exit_code: None,
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(
            !s.contains("\"command\""),
            "command field should be omitted when None, got: {}",
            s
        );
    }

    // --- exit_code lifecycle wiring (#8) -------------------------

    #[test]
    fn teammate_pane_info_omits_exit_code_when_none() {
        // Default state for a live (or freshly-spawned) teammate: no
        // exit code yet. The field must not appear on the wire so
        // existing `team.list` consumers that don't know about
        // `exit_code` are unaffected.
        let t = TeammatePaneInfo {
            agent_id: "a".into(),
            pane_id: 1,
            name: "n".into(),
            command: None,
            exited: false,
            exit_code: None,
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(
            !s.contains("\"exit_code\""),
            "exit_code field should be omitted when None, got: {}",
            s
        );
    }

    #[test]
    fn teammate_pane_info_serializes_exit_code_when_some() {
        // After `CommandPaneExited` the plugin sets `exited = true`
        // and stores the exit code. Both fields must round-trip on
        // the wire so the shim (and any future consumer) can render
        // the post-mortem status of a teammate pane.
        let t = TeammatePaneInfo {
            agent_id: "a".into(),
            pane_id: 1,
            name: "n".into(),
            command: None,
            exited: true,
            exit_code: Some(0),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""exited":true"#), "got: {}", s);
        assert!(s.contains(r#""exit_code":0"#), "got: {}", s);
    }

    #[test]
    fn teammate_pane_info_serializes_nonzero_exit_code() {
        // Non-zero exit codes are the interesting case for a shim
        // that wants to surface failures to the operator.
        let t = TeammatePaneInfo {
            agent_id: "a".into(),
            pane_id: 1,
            name: "n".into(),
            command: None,
            exited: true,
            exit_code: Some(137),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""exit_code":137"#), "got: {}", s);
    }

    #[test]
    fn teammate_pane_info_serializes_negative_exit_code() {
        // i32 is the type Zellij hands us; signed values are
        // possible (e.g. a -1 sentinel) so the wire format must
        // round-trip them faithfully.
        let t = TeammatePaneInfo {
            agent_id: "a".into(),
            pane_id: 1,
            name: "n".into(),
            command: None,
            exited: true,
            exit_code: Some(-1),
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains(r#""exit_code":-1"#), "got: {}", s);
    }

    #[test]
    fn teammate_pane_info_round_trips() {
        // The wire-compat reviewer on PR #54 flagged that prior tests
        // were serialize-only substring asserts. With `Deserialize`
        // now derived on `TeammatePaneInfo`, verify the full
        // round-trip preserves every field — both the "live"
        // (exited=false, exit_code=None) and "exited" (exited=true,
        // exit_code=Some(N)) shapes.
        for original in [
            TeammatePaneInfo {
                agent_id: "researcher@team".into(),
                pane_id: 42,
                name: "researcher".into(),
                command: Some("claude --agent-id researcher@team".into()),
                exited: false,
                exit_code: None,
            },
            TeammatePaneInfo {
                agent_id: "linter@team".into(),
                pane_id: 99,
                name: "linter".into(),
                command: None,
                exited: true,
                exit_code: Some(137),
            },
        ] {
            let s = serde_json::to_string(&original).unwrap();
            let parsed: TeammatePaneInfo = serde_json::from_str(&s).expect("round-trip parse");
            assert_eq!(original, parsed, "round-trip mismatch via: {}", s);
        }
    }

    #[test]
    fn team_list_result_round_trips_mixed_state() {
        // `team.list` returns a `TeamListResult` whose `panes` may
        // mix live and exited teammates. Verify the full result type
        // round-trips — closing the wire-compat reviewer's coverage
        // gap on the result wrapper, not just the inner element type.
        let original = TeamListResult {
            panes: vec![
                TeammatePaneInfo {
                    agent_id: "researcher@team".into(),
                    pane_id: 1,
                    name: "researcher".into(),
                    command: Some("claude ...".into()),
                    exited: false,
                    exit_code: None,
                },
                TeammatePaneInfo {
                    agent_id: "linter@team".into(),
                    pane_id: 2,
                    name: "linter".into(),
                    command: None,
                    exited: true,
                    exit_code: Some(0),
                },
                TeammatePaneInfo {
                    agent_id: "killed@team".into(),
                    pane_id: 3,
                    name: "killed".into(),
                    command: Some("claude ...".into()),
                    exited: true,
                    exit_code: Some(-1),
                },
            ],
        };
        let s = serde_json::to_string(&original).unwrap();
        let parsed: TeamListResult = serde_json::from_str(&s).expect("TeamListResult round-trip");
        assert_eq!(
            original, parsed,
            "TeamListResult round-trip mismatch via: {}",
            s
        );
    }
}
