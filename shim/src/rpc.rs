//! JSON-RPC client over `zellij pipe`.
//!
//! Every shim subcommand that interacts with the plugin goes through
//! [`call`]: serialize a [`roger_proto::Request`] into the payload of a
//! `zellij pipe --name roger-rpc` invocation, read the plugin's reply
//! from the child's stdout, parse it as a [`roger_proto::Response`],
//! and return either the result value or a typed [`RpcError`].
//!
//! Protocol reference: `docs/rpc-protocol.md`.

use std::io::{self, Write};
use std::process::{Command, Stdio};

use roger_proto::{error_codes, Request, Response};
use serde::de::DeserializeOwned;
use serde_json::Value;

/// Name passed to `zellij pipe --name`. Must match what the plugin
/// expects (the plugin doesn't actually filter by name, but we keep
/// it stable for log readability and future hardening).
const PIPE_NAME: &str = "roger-rpc";

/// Errors surfaced by [`call`]. Each variant maps to a distinct exit
/// shape on the shim side: `SpawnFailed` and `Io` are environment
/// problems (Zellij not running, binary not found); `Protocol`
/// indicates the plugin replied with something we couldn't parse;
/// `Rpc` is the plugin returning a structured error code.
#[derive(Debug)]
pub enum RpcError {
    /// Failed to spawn `zellij pipe` (e.g. the binary isn't on PATH).
    SpawnFailed(io::Error),
    /// I/O failed mid-call (writing the request, reading the reply).
    Io(io::Error),
    /// `zellij pipe` exited non-zero.
    PipeExitFailure { status: i32, stderr: String },
    /// The reply parsed as JSON but not as a `Response`, or both
    /// `result` and `error` were absent.
    Protocol(String),
    /// The plugin returned a structured error response.
    Rpc { code: i32, message: String },
    /// The reply's `id` did not match the request's `id`. Should never
    /// happen since the plugin echoes verbatim; if it does, treat as
    /// fatal because the reply doesn't belong to this caller.
    IdMismatch { sent: String, received: String },
    /// The plugin returned a `result` but it didn't deserialize into
    /// the expected handler-specific type.
    DeserializeResult(serde_json::Error),
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpawnFailed(e) => write!(f, "spawn `zellij pipe`: {}", e),
            Self::Io(e) => write!(f, "i/o during RPC: {}", e),
            Self::PipeExitFailure { status, stderr } => {
                write!(f, "`zellij pipe` exited {}: {}", status, stderr.trim())
            }
            Self::Protocol(msg) => write!(f, "protocol error: {}", msg),
            Self::Rpc { code, message } => write!(f, "rpc error {}: {}", code, message),
            Self::IdMismatch { sent, received } => {
                write!(f, "rpc id mismatch (sent {:?}, got {:?})", sent, received)
            }
            Self::DeserializeResult(e) => write!(f, "could not parse rpc result: {}", e),
        }
    }
}

impl std::error::Error for RpcError {}

/// Make a synchronous JSON-RPC call to the plugin.
///
/// `method` is the dotted method name (`team.list`, `team.spawn`,
/// `team.send`, `team.kill`). `params` is a `serde_json::Value` —
/// callers usually build it via the `serde_json::json!` macro. The
/// `id` is generated internally (UUID v4) and echoed by the plugin;
/// callers don't need to thread it through.
pub fn call<R: DeserializeOwned>(method: &str, params: Value) -> Result<R, RpcError> {
    let id = uuid::Uuid::new_v4().to_string();
    let request = Request {
        method: method.to_string(),
        id: id.clone(),
        params,
    };
    let body = serde_json::to_string(&request)
        .expect("Request is composed of String + Value; serialization cannot fail");

    let response = invoke_pipe(&body)?;
    let parsed: Response = serde_json::from_str(&response)
        .map_err(|e| RpcError::Protocol(format!("response was not valid JSON-RPC: {}", e)))?;

    if parsed.id != id {
        return Err(RpcError::IdMismatch {
            sent: id,
            received: parsed.id,
        });
    }

    match (parsed.result, parsed.error) {
        (Some(value), None) => serde_json::from_value(value).map_err(RpcError::DeserializeResult),
        (None, Some(err)) => Err(RpcError::Rpc {
            code: err.code,
            message: err.message,
        }),
        (None, None) => Err(RpcError::Protocol(
            "response had neither `result` nor `error`".to_string(),
        )),
        (Some(_), Some(_)) => Err(RpcError::Protocol(
            "response had both `result` and `error`".to_string(),
        )),
    }
}

/// Spawn `zellij pipe`, write the request body to its stdin, return
/// stdout. Separated from [`call`] to keep the JSON envelope plumbing
/// distinct from the subprocess plumbing.
fn invoke_pipe(body: &str) -> Result<String, RpcError> {
    let mut child = Command::new("zellij")
        .arg("pipe")
        .arg("--name")
        .arg(PIPE_NAME)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(RpcError::SpawnFailed)?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| RpcError::Io(io::Error::other("zellij pipe stdin missing")))?;
        stdin.write_all(body.as_bytes()).map_err(RpcError::Io)?;
        // Dropping `stdin` at the end of this scope closes the fd,
        // which signals end-of-payload to `zellij pipe`.
    }

    let output = child.wait_with_output().map_err(RpcError::Io)?;
    if !output.status.success() {
        return Err(RpcError::PipeExitFailure {
            status: output.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Convenience: classify an [`RpcError`] as
/// `unknown pane_id` (which the plugin returns for stale `team.send`
/// / `team.kill` targets). Used by handler-specific exit-code
/// decisions so a kill-against-already-closed-pane doesn't have to
/// look like a hard error.
pub fn is_unknown_pane_id(err: &RpcError) -> bool {
    matches!(
        err,
        RpcError::Rpc { code, message }
            if *code == error_codes::INVALID_PARAMS && message == "unknown pane_id"
    )
}
