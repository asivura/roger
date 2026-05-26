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
/// Spawn / lifecycle wiring lands in #6 and #8.
#[derive(Debug, Clone, Serialize)]
pub struct TeammatePaneInfo {
    pub agent_id: String,
    pub pane_id: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub exited: bool,
}

/// Result type for the `team.list` method.
#[derive(Debug, Serialize)]
pub struct TeamListResult {
    pub panes: Vec<TeammatePaneInfo>,
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
    // Referenced from tests below (where it's "used") but not yet from
    // the plugin's runtime code; clippy's dead-code lint runs on
    // non-test builds only, hence the cfg gate. Removed once
    // `team.spawn` (#6) emits this.
    #[cfg_attr(not(test), allow(dead_code))]
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
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

    #[test]
    fn teammate_pane_info_omits_command_when_none() {
        let t = TeammatePaneInfo {
            agent_id: "a".into(),
            pane_id: 1,
            name: "n".into(),
            command: None,
            exited: false,
        };
        let s = serde_json::to_string(&t).unwrap();
        assert!(
            !s.contains("\"command\""),
            "command field should be omitted when None, got: {}",
            s
        );
    }
}
