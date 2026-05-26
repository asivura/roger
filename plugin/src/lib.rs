//! roger — a Zellij plugin for orchestrating Claude Code agent teams as
//! native panes.
//!
//! Named after Roger Penrose; see README.md for the full story.
//!
//! This file is the plugin entrypoint. It registers the plugin, requests
//! the permissions the RPC layer needs, subscribes to pane lifecycle
//! events, and dispatches `zellij pipe` payloads to method handlers
//! defined here and in [`rpc`].
//!
//! Implemented methods today (PR #5):
//!   - `team.list`
//!
//! Planned (subsequent PRs):
//!   - `team.spawn` (#6), `team.send` + `team.kill` (#7)
//!   - lifecycle wiring (#8) that populates the state map
//!     `CommandPaneOpened` / `CommandPaneExited` react to.

mod rpc;

use std::collections::{BTreeMap, HashMap};

use zellij_tile::prelude::*;

use rpc::{error_codes, Request, Response, TeamListResult, TeammatePaneInfo};

#[derive(Default)]
struct State {
    /// Map from agent identifier (e.g. `"researcher@my-team"`) to the
    /// pane currently hosting that teammate, plus its mutable status.
    /// Populated by `team.spawn` (#6); read by `team.list` (this PR).
    teammates: HashMap<String, TeammatePaneInfo>,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        // Permissions roger will eventually need. Requested up front so
        // the user only confirms once, even though some are unused in v0.
        request_permission(&[
            // Read the team config files, list panes, inspect state.
            PermissionType::ReadApplicationState,
            // Open / close / rename / focus panes, change layouts.
            PermissionType::ChangeApplicationState,
            // Spawn `claude` (and friends) into new panes.
            PermissionType::OpenTerminalsOrPlugins,
            // `run_command` escape hatch for reading
            // `~/.claude/teams/<team>/config.json` (the Wasm sandbox
            // does not expose arbitrary filesystem paths).
            PermissionType::RunCommands,
            // Send keystrokes / text into teammate panes (the inner
            // half of TmuxBackend's `send-keys` semantics).
            PermissionType::WriteToStdin,
            // Accept RPC over `zellij pipe`.
            PermissionType::ReadCliPipes,
            // TODO: `PermissionType::ReadPaneContents` was added in
            // zellij-tile 0.44; once we bump the dep, request it so we
            // can capture pane scrollback for the observability
            // surface (status, "what did this teammate just do"
            // hover, etc).
        ]);

        // Pane lifecycle events. The shim CLI tells us *what* to spawn;
        // these tell us when the resulting process has actually
        // started, exited (with what code), or been closed. Handlers
        // for these land in #8.
        subscribe(&[
            EventType::PaneUpdate,
            EventType::CommandPaneOpened,
            EventType::CommandPaneExited,
            EventType::CommandPaneReRun,
            EventType::PaneClosed,
        ]);

        // roger has no UI of its own; it lives entirely in the
        // permission-granted background.
        hide_self();
    }

    fn update(&mut self, event: Event) -> bool {
        // Lifecycle wiring lands in #8. For now we only log so an
        // operator can confirm the events are flowing.
        match event {
            Event::CommandPaneOpened(pane_id, _ctx) => {
                eprintln!("[roger] CommandPaneOpened pane_id={}", pane_id);
            }
            Event::CommandPaneExited(pane_id, exit_code, _ctx) => {
                eprintln!(
                    "[roger] CommandPaneExited pane_id={} exit_code={:?}",
                    pane_id, exit_code
                );
            }
            Event::PaneClosed(pane_id) => {
                eprintln!("[roger] PaneClosed pane_id={:?}", pane_id);
            }
            _ => {}
        }
        false
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        // We only respond to CLI pipes (the shim's invocation path).
        // Plugin-to-plugin and keybind pipes don't block on us, so
        // there's no `unblock_cli_pipe_input` obligation for them.
        //
        // Once we match `PipeSource::Cli(_)` below, the CLI side is
        // BLOCKED until we reply. Every path from that point on MUST
        // call `reply(...)` (which writes via cli_pipe_output then
        // unblock_cli_pipe_input) before returning, or the shim
        // hangs forever.
        let pipe_id = match &pipe_message.source {
            PipeSource::Cli(id) => id.clone(),
            other => {
                eprintln!("[roger] non-CLI pipe ignored: {:?}", other);
                return false;
            }
        };

        let response = self.handle_pipe(&pipe_message);
        reply(&pipe_id, &response);
        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        // Hidden plugin: nothing to render.
    }
}

impl State {
    /// Two-stage pipe-payload parse. Splits responsibility so we can:
    ///   (a) detect a totally-malformed body (not JSON) and return
    ///       `PARSE_ERROR` with `id=""` (no id to echo);
    ///   (b) detect a JSON-but-wrong-shape body, salvage the `id`
    ///       field if present, and return `INVALID_REQUEST` with the
    ///       salvaged id (protocol contract: "id is echoed verbatim").
    ///
    /// The second case used to collapse into `PARSE_ERROR` with
    /// `id=""`, leaking the shim's pending-request entry. Reviewers
    /// flagged this on PR #35 (correctness + error-handling, both
    /// Important).
    fn handle_pipe(&self, pipe_message: &PipeMessage) -> Response {
        let payload = match pipe_message.payload.as_deref() {
            Some(p) if !p.is_empty() => p,
            _ => return Response::err("", error_codes::PARSE_ERROR, "empty payload"),
        };

        let raw: serde_json::Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(e) => {
                return Response::err("", error_codes::PARSE_ERROR, format!("invalid JSON: {}", e))
            }
        };

        // Salvage `id` BEFORE attempting the typed shape, so a
        // wrong-shape body still gets its id echoed back.
        let salvaged_id = raw
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let request: Request = match serde_json::from_value(raw) {
            Ok(r) => r,
            Err(e) => {
                return Response::err(
                    &salvaged_id,
                    error_codes::INVALID_REQUEST,
                    format!("invalid request shape: {}", e),
                )
            }
        };

        self.dispatch(&request)
    }

    fn dispatch(&self, request: &Request) -> Response {
        match request.method.as_str() {
            "team.list" => self.handle_team_list(request),
            other => Response::err(
                &request.id,
                error_codes::METHOD_NOT_FOUND,
                format!("method not found: {}", other),
            ),
        }
    }

    /// `team.list` returns the panes currently tracked by roger.
    ///
    /// The map is populated by `team.spawn` (#6) and updated by the
    /// lifecycle event handlers in `update()` (#8). In this PR the map
    /// is always empty, so `team.list` returns `{"panes": []}` until
    /// the rest of the protocol lands. That's fine — the shim wants
    /// to know the plugin is alive and reachable, and an empty list
    /// is a valid answer for a freshly-loaded session.
    fn handle_team_list(&self, request: &Request) -> Response {
        let panes: Vec<TeammatePaneInfo> = self.teammates.values().cloned().collect();
        let result = TeamListResult { panes };
        match serde_json::to_value(&result) {
            Ok(value) => Response::ok(&request.id, value),
            Err(e) => Response::err(
                &request.id,
                error_codes::INTERNAL_ERROR,
                format!("serialize team.list result: {}", e),
            ),
        }
    }
}

/// Write a JSON response to the CLI pipe and unblock the caller.
///
/// The fallback path (if `serde_json::to_string(response)` itself
/// fails) builds the error JSON by hand, escaping the inner error
/// message through `serde_json` so the fallback is *always* valid
/// JSON. Earlier code used a `format!` that embedded the raw error
/// text, which could produce invalid JSON if the message contained
/// quotes / newlines / non-ASCII (reviewer-flagged, PR #35).
fn reply(pipe_id: &str, response: &Response) {
    let body = serde_json::to_string(response).unwrap_or_else(|e| {
        let safe_message = serde_json::to_string(&format!("failed to serialize response: {}", e))
            .unwrap_or_else(|_| "\"internal error\"".to_string());
        format!(
            r#"{{"id":"","error":{{"code":{},"message":{}}}}}"#,
            error_codes::INTERNAL_ERROR,
            safe_message
        )
    });
    cli_pipe_output(pipe_id, &body);
    unblock_cli_pipe_input(pipe_id);
}
