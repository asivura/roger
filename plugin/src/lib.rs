//! roger — a Zellij plugin for orchestrating Claude Code agent teams as
//! native panes.
//!
//! Named after Roger Penrose; see README.md for the full story.
//!
//! This file is the plugin entrypoint. It registers the plugin, requests
//! the permissions the RPC layer needs, subscribes to pane lifecycle
//! events, and dispatches `zellij pipe` payloads to method handlers.
//!
//! Implemented methods:
//!   - `team.list` (PR #35)
//!   - `team.spawn` (PR #44)
//!   - `team.send` + `team.kill` (this PR — #7)
//!
//! Planned:
//!   - lifecycle wiring (#8) — `CommandPaneExited` marks teammate
//!     `exited: true`, `PaneClosed` removes from `State::teammates`
//!
//! ## team.spawn: deferred-reply pattern
//!
//! `open_command_pane` in `zellij-tile 0.43.1` returns `()` — the
//! resulting pane id arrives asynchronously via the `CommandPaneOpened`
//! event. To honor the synchronous shim → plugin RPC (the shim's
//! `zellij pipe` call blocks until we reply), we:
//!
//! 1. In `pipe()`, parse the spawn request, generate a correlation
//!    token (we reuse `request.id`), insert a `PendingSpawn` into
//!    `State::pending_spawns`, then call `open_command_pane` with the
//!    token in its `Context` map.
//! 2. Return from `pipe()` WITHOUT replying. The CLI side stays
//!    blocked.
//! 3. When `CommandPaneOpened(pane_id, ctx)` fires in `update()`, we
//!    extract the token from `ctx`, look up the pending spawn, insert
//!    a `TeammatePaneInfo` into `State::teammates`, and call `reply()`
//!    — which unblocks the CLI.
//!
//! Limitation (deferred to a follow-up): no timeout. If
//! `CommandPaneOpened` never arrives (e.g. argv refers to a missing
//! binary), the shim hangs until its own client-side timeout fires.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use zellij_tile::prelude::*;

use roger_proto::{
    error_codes, KillParams, OkResult, Request, Response, SendParams, SpawnParams, SpawnResult,
    TeamListResult, TeammatePaneInfo,
};

/// Key used in `CommandToRun`'s `Context` map to carry the per-spawn
/// correlation token from `pipe()` (where we open the pane) to
/// `update()` (where we get the resulting pane id and reply).
const SPAWN_TOKEN_KEY: &str = "roger.spawn_token";

#[derive(Default)]
struct State {
    /// Currently-tracked teammate panes, keyed by Zellij *terminal*
    /// pane id (`PaneId::Terminal(u32)` per zellij-utils 0.43.1; the
    /// `u32` is what arrives in `CommandPaneOpened`). The shim
    /// addresses subsequent `team.send` / `team.kill` calls by pane
    /// id (the value it received from `team.spawn`), so keying by
    /// pane id makes those lookups direct. Populated by the
    /// `CommandPaneOpened` handler after a `team.spawn` succeeds.
    ///
    /// **Invariant:** the only insertion site is
    /// `on_command_pane_opened`. `team.send` and `team.kill` rely on
    /// this to be safe in the face of Zellij pane-id reuse: if a
    /// terminal pane closes and Zellij later assigns the same numeric
    /// id to a non-Roger pane, that pane won't be in `teammates`, so
    /// the membership check rejects the operation. Don't add insertion
    /// sites elsewhere without revisiting the security review on #50.
    teammates: HashMap<u32, TeammatePaneInfo>,
    /// In-flight `team.spawn` calls awaiting `CommandPaneOpened`,
    /// keyed by a plugin-internal correlation token (see
    /// `next_spawn_token`). We do NOT reuse `request.id` as the key
    /// because the shim is allowed to repeat ids — colliding ids
    /// would silently drop the first caller's `PendingSpawn` and
    /// route the wrong response. (correctness reviewer, PR #44.)
    pending_spawns: HashMap<u64, PendingSpawn>,
    /// Monotonic counter for spawn correlation tokens. Wraps at
    /// `u64::MAX` which is fine — the universe ends before we hit it.
    next_spawn_token: u64,
}

/// Context the plugin remembers while a `team.spawn` is in flight,
/// so the `CommandPaneOpened` handler can finish the RPC.
struct PendingSpawn {
    pipe_id: String,
    request_id: String,
    agent_id: String,
    name: String,
    command: String,
}

/// What `handle_pipe` returns to `pipe()`.
///
/// Most methods reply immediately (`Reply`). `team.spawn` defers the
/// reply to the `CommandPaneOpened` event handler (`Deferred`); the
/// CLI side stays blocked until that handler calls `reply()`.
enum DispatchOutcome {
    Reply(Response),
    Deferred,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::OpenTerminalsOrPlugins,
            PermissionType::RunCommands,
            PermissionType::WriteToStdin,
            PermissionType::ReadCliPipes,
            // TODO: `PermissionType::ReadPaneContents` was added in
            // zellij-tile 0.44; once we bump the dep (#13), request it
            // for the observability surface.
        ]);

        subscribe(&[
            EventType::PaneUpdate,
            EventType::CommandPaneOpened,
            EventType::CommandPaneExited,
            EventType::CommandPaneReRun,
            EventType::PaneClosed,
        ]);

        hide_self();
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::CommandPaneOpened(pane_id, ctx) => {
                self.on_command_pane_opened(pane_id, &ctx);
            }
            Event::CommandPaneExited(pane_id, exit_code, _ctx) => {
                // Full lifecycle handling (mark `exited: true`, log
                // exit_code) lands in #8.
                eprintln!(
                    "[roger] CommandPaneExited pane_id={} exit_code={:?}",
                    pane_id, exit_code
                );
            }
            Event::PaneClosed(pane_id) => {
                // #8 will remove from `self.teammates` here.
                eprintln!("[roger] PaneClosed pane_id={:?}", pane_id);
            }
            _ => {}
        }
        false
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        // Once we match `PipeSource::Cli`, the CLI is BLOCKED until we
        // reply. Every path below MUST either call `reply(...)` or
        // return `DispatchOutcome::Deferred` (in which case the event
        // handler is responsible for the reply).
        let pipe_id = match &pipe_message.source {
            PipeSource::Cli(id) => id.clone(),
            other => {
                eprintln!("[roger] non-CLI pipe ignored: {:?}", other);
                return false;
            }
        };

        match self.handle_pipe(&pipe_id, &pipe_message) {
            DispatchOutcome::Reply(response) => reply(&pipe_id, &response),
            DispatchOutcome::Deferred => {
                // Reply will be sent by the event handler when the
                // spawn completes. Do not touch the pipe here.
            }
        }
        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        // Hidden plugin: nothing to render.
    }
}

impl State {
    /// Two-stage parse so the `id` survives a wrong-shape body and we
    /// can emit the right error code (`PARSE_ERROR` for non-JSON,
    /// `INVALID_REQUEST` for JSON-but-wrong-shape). PR #35 review
    /// (correctness + error-handling) called this out as Important.
    fn handle_pipe(&mut self, pipe_id: &str, pipe_message: &PipeMessage) -> DispatchOutcome {
        let payload = match pipe_message.payload.as_deref() {
            Some(p) if !p.is_empty() => p,
            _ => {
                return DispatchOutcome::Reply(Response::err(
                    "",
                    error_codes::PARSE_ERROR,
                    "empty payload",
                ));
            }
        };

        let raw: serde_json::Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(e) => {
                return DispatchOutcome::Reply(Response::err(
                    "",
                    error_codes::PARSE_ERROR,
                    format!("invalid JSON: {}", e),
                ));
            }
        };

        let salvaged_id = raw
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let request: Request = match serde_json::from_value(raw) {
            Ok(r) => r,
            Err(e) => {
                return DispatchOutcome::Reply(Response::err(
                    &salvaged_id,
                    error_codes::INVALID_REQUEST,
                    format!("invalid request shape: {}", e),
                ));
            }
        };

        self.dispatch(pipe_id, &request)
    }

    fn dispatch(&mut self, pipe_id: &str, request: &Request) -> DispatchOutcome {
        match request.method.as_str() {
            "team.list" => DispatchOutcome::Reply(self.handle_team_list(request)),
            "team.spawn" => self.handle_team_spawn(pipe_id, request),
            "team.send" => DispatchOutcome::Reply(self.handle_team_send(request)),
            "team.kill" => DispatchOutcome::Reply(self.handle_team_kill(request)),
            other => DispatchOutcome::Reply(Response::err(
                &request.id,
                error_codes::METHOD_NOT_FOUND,
                format!("method not found: {}", other),
            )),
        }
    }

    /// `team.list` — synchronous, just serializes the state map.
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

    /// `team.spawn` — async via the correlation-token pattern. See the
    /// module doc comment for the full flow.
    fn handle_team_spawn(&mut self, pipe_id: &str, request: &Request) -> DispatchOutcome {
        let params: SpawnParams = match serde_json::from_value(request.params.clone()) {
            Ok(p) => p,
            Err(e) => {
                return DispatchOutcome::Reply(Response::err(
                    &request.id,
                    error_codes::INVALID_PARAMS,
                    format!("invalid team.spawn params: {}", e),
                ));
            }
        };

        if params.argv.is_empty() {
            return DispatchOutcome::Reply(Response::err(
                &request.id,
                error_codes::SPAWN_FAILED,
                "team.spawn: argv is empty",
            ));
        }

        // Mint a fresh internal token. Do NOT reuse `request.id` —
        // the shim is allowed to repeat ids and colliding ids would
        // silently drop a caller's PendingSpawn (correctness review,
        // PR #44). The protocol id is still echoed back via
        // PendingSpawn::request_id.
        let token = self.next_spawn_token;
        self.next_spawn_token = self.next_spawn_token.wrapping_add(1);
        let command_display = params.argv.join(" ");

        let cmd = CommandToRun {
            path: PathBuf::from(&params.argv[0]),
            args: params.argv[1..].to_vec(),
            cwd: Some(PathBuf::from(&params.cwd)),
        };

        self.pending_spawns.insert(
            token,
            PendingSpawn {
                pipe_id: pipe_id.to_string(),
                request_id: request.id.clone(),
                agent_id: params.agent_id,
                name: params.name,
                command: command_display,
            },
        );

        let mut ctx = BTreeMap::new();
        ctx.insert(SPAWN_TOKEN_KEY.to_string(), token.to_string());
        open_command_pane(cmd, ctx);

        // Reply is deferred to `on_command_pane_opened` in update().
        // If the shim closes the pipe before then (timeout, SIGINT),
        // the eventual cli_pipe_output/unblock_cli_pipe_input calls
        // silently no-op on the host side — verified safe in
        // zellij-tile 0.43.1, no panic, no abort (error-handling
        // reviewer, PR #44).
        DispatchOutcome::Deferred
    }

    /// Called from `update()` when a Zellij pane finishes opening.
    /// Looks up the pending spawn by correlation token, inserts the
    /// teammate into `State::teammates`, and replies on the CLI pipe.
    fn on_command_pane_opened(&mut self, pane_id: u32, ctx: &BTreeMap<String, String>) {
        let token_str = match ctx.get(SPAWN_TOKEN_KEY) {
            Some(t) => t,
            None => {
                // A `CommandPaneOpened` we didn't initiate (e.g. the
                // user opened a pane manually). Nothing to do.
                eprintln!(
                    "[roger] CommandPaneOpened pane_id={} (no roger token; ignored)",
                    pane_id
                );
                return;
            }
        };
        let token: u64 = match token_str.parse() {
            Ok(t) => t,
            Err(_) => {
                eprintln!(
                    "[roger] CommandPaneOpened pane_id={} unparseable token={:?}; ignored",
                    pane_id, token_str
                );
                return;
            }
        };

        let pending = match self.pending_spawns.remove(&token) {
            Some(p) => p,
            None => {
                // Token present but we don't remember this spawn.
                // Possible race / restart; log and drop.
                eprintln!(
                    "[roger] CommandPaneOpened pane_id={} token={} not in pending_spawns",
                    pane_id, token
                );
                return;
            }
        };

        self.teammates.insert(
            pane_id,
            TeammatePaneInfo {
                agent_id: pending.agent_id,
                pane_id,
                name: pending.name,
                command: Some(pending.command),
                exited: false,
            },
        );

        let result = match serde_json::to_value(SpawnResult { pane_id }) {
            Ok(v) => v,
            Err(e) => {
                reply(
                    &pending.pipe_id,
                    &Response::err(
                        &pending.request_id,
                        error_codes::INTERNAL_ERROR,
                        format!("serialize team.spawn result: {}", e),
                    ),
                );
                return;
            }
        };
        reply(&pending.pipe_id, &Response::ok(&pending.request_id, result));
    }

    /// `team.send` — write text into a tracked teammate pane's PTY.
    ///
    /// Synchronous at the JSON-RPC layer: we reply as soon as the
    /// write is dispatched to Zellij. The underlying
    /// `write_chars_to_pane_id` is fire-and-forget host-side
    /// (`zellij-tile 0.43.1` returns `()` with no acknowledgement),
    /// so `{ "ok": true }` means *the plugin handed the bytes to
    /// Zellij*, not *the bytes reached the PTY*. The most plausible
    /// failure mode is a write swallowed by a pane that exited
    /// between the membership check and the host call — harmless,
    /// since #8's `PaneClosed` cleanup will remove the entry shortly.
    fn handle_team_send(&self, request: &Request) -> Response {
        let params: SendParams = match serde_json::from_value(request.params.clone()) {
            Ok(p) => p,
            Err(e) => {
                return Response::err(
                    &request.id,
                    error_codes::INVALID_PARAMS,
                    format!("invalid team.send params: {}", e),
                );
            }
        };

        if !self.teammates.contains_key(&params.pane_id) {
            // Don't echo `params.pane_id` in the message — the caller
            // already knows the value they sent. Keeping the response
            // value-free removes a behavioral oracle that a future
            // authz layer would otherwise inherit by accident
            // (security review, PR #50).
            return Response::err(
                &request.id,
                error_codes::INVALID_PARAMS,
                "unknown pane_id".to_string(),
            );
        }

        write_chars_to_pane_id(&params.text, PaneId::Terminal(params.pane_id));

        match serde_json::to_value(OkResult { ok: true }) {
            Ok(v) => Response::ok(&request.id, v),
            Err(e) => Response::err(
                &request.id,
                error_codes::INTERNAL_ERROR,
                format!("serialize team.send result: {}", e),
            ),
        }
    }

    /// `team.kill` — close a tracked teammate pane and remove it
    /// from `State::teammates`. Optimistic removal: we drop the
    /// entry immediately rather than waiting for `PaneClosed`, since
    /// the shim explicitly asked for the kill and the call is
    /// fire-and-forget. The eventual `PaneClosed` event will find no
    /// matching entry and harmlessly no-op (#8 wiring).
    fn handle_team_kill(&mut self, request: &Request) -> Response {
        let params: KillParams = match serde_json::from_value(request.params.clone()) {
            Ok(p) => p,
            Err(e) => {
                return Response::err(
                    &request.id,
                    error_codes::INVALID_PARAMS,
                    format!("invalid team.kill params: {}", e),
                );
            }
        };

        if self.teammates.remove(&params.pane_id).is_none() {
            // Same rationale as `handle_team_send`: don't echo the
            // caller-supplied pane id back (security review, PR #50).
            return Response::err(
                &request.id,
                error_codes::INVALID_PARAMS,
                "unknown pane_id".to_string(),
            );
        }

        close_pane_with_id(PaneId::Terminal(params.pane_id));

        match serde_json::to_value(OkResult { ok: true }) {
            Ok(v) => Response::ok(&request.id, v),
            Err(e) => Response::err(
                &request.id,
                error_codes::INTERNAL_ERROR,
                format!("serialize team.kill result: {}", e),
            ),
        }
    }
}

/// Write a JSON response to the CLI pipe and unblock the caller.
///
/// The fallback path (if `serde_json::to_string(response)` itself
/// fails) builds the error JSON by hand, escaping the inner error
/// message through `serde_json` so the fallback is *always* valid
/// JSON (PR #35 review).
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
