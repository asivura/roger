//! JSON-RPC-style protocol used between the `roger-shim` CLI and this
//! plugin, transported over Zellij CLI pipes.
//!
//! Full protocol reference: `docs/rpc-protocol.md`.
//!
//! Method surface (#5 ships `team.list` only; #6-#7 land the others):
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
/// `team.list` (this PR) takes no params and never reads the field;
/// `team.spawn` (#6) and `team.send` / `team.kill` (#7) will.
#[derive(Debug, Deserialize)]
pub struct Request {
    pub method: String,
    pub id: String,
    #[serde(default)]
    #[allow(dead_code)] // used by team.spawn / team.send / team.kill (#6, #7)
    pub params: serde_json::Value,
}

/// Response envelope. Exactly one of `result` / `error` is set;
/// the other is omitted from the serialized form via `skip_serializing_if`.
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
/// only `PARSE_ERROR`, `METHOD_NOT_FOUND`, and `INTERNAL_ERROR` are
/// referenced in this PR — they'll be used by `team.spawn` (#6) and
/// `team.send` / `team.kill` (#7).
#[allow(dead_code)]
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
}
